//! Elicitation: a server asks the user for input (MCP 2025-11-25,
//! client/elicitation).
//!
//! The pure part: read the request, read and check each answer, and make
//! the result. The command line asks the questions on a TTY.

use serde_json::{Map, Value, json};

/// The JSON-RPC error code of a request that needs the user to open a
/// page first (`URLElicitationRequiredError`, MCP 2025-11-25).
pub const URL_REQUIRED: i32 = -32042;

/// A page that the server asks the user to open, from the data of a
/// `URLElicitationRequiredError`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub message: String,
    pub url: String,
}

/// The pages in the data of a `URLElicitationRequiredError`. A page that
/// is safe to open (`safe_url`) has its URL in normal form; another page
/// gives the reason that `ncp` refuses it. An item with no URL is left out.
#[must_use]
pub fn pages(data: Option<&Value>) -> Vec<Result<Page, String>> {
    data.and_then(|data| data.get("elicitations"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let url = safe_url(&text(item, "url")?);
                    Some(url.map(|url| Page {
                        message: text(item, "message").unwrap_or_default(),
                        url: url.to_string(),
                    }))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// What a server asks.
#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    /// Values for a flat form of primitive fields.
    Form { message: String, fields: Vec<Field> },
    /// The user opens a URL, for example to give a credential to the
    /// server directly. The client never sees that value.
    Url { message: String, url: String },
}

/// One field of a form.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub name: String,
    /// The `title` of the field, else its name.
    pub label: String,
    pub description: Option<String>,
    pub kind: Kind,
    pub required: bool,
    /// The `default` of the schema. An empty answer takes it.
    pub default: Option<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    Text {
        min_length: Option<u64>,
        max_length: Option<u64>,
        format: Option<TextFormat>,
    },
    Integer {
        minimum: Option<f64>,
        maximum: Option<f64>,
    },
    Number {
        minimum: Option<f64>,
        maximum: Option<f64>,
    },
    Boolean,
    /// One value of a list: `enum`, or `oneOf` with `const` and `title`.
    Choice(Vec<Choice>),
    /// Values of a list: an array of `enum` or `anyOf` items.
    Choices {
        list: Vec<Choice>,
        min_items: Option<u64>,
        max_items: Option<u64>,
    },
}

/// The formats of a text field that the spec names. The check is a light
/// one, to catch a typing error: the server checks the value again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextFormat {
    Email,
    Uri,
    Date,
    DateTime,
}

impl TextFormat {
    fn parse(name: &str) -> Option<Self> {
        match name {
            "email" => Some(Self::Email),
            "uri" => Some(Self::Uri),
            "date" => Some(Self::Date),
            "date-time" => Some(Self::DateTime),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Email => "an email address",
            Self::Uri => "a URI",
            Self::Date => "a date, YYYY-MM-DD",
            Self::DateTime => "a date and time, YYYY-MM-DDThh:mm:ssZ",
        }
    }

    fn accepts(self, text: &str) -> bool {
        match self {
            Self::Email => text
                .rsplit_once('@')
                .is_some_and(|(local, domain)| !local.is_empty() && !domain.is_empty()),
            Self::Uri => url::Url::parse(text).is_ok(),
            Self::Date => is_date(text),
            Self::DateTime => {
                text.get(..10).is_some_and(is_date)
                    && text
                        .get(10..11)
                        .is_some_and(|t| t.eq_ignore_ascii_case("t"))
                    && text.get(11..19).is_some_and(is_time)
                    && text.get(19..).is_some_and(is_offset)
            }
        }
    }
}

/// A time as `hh:mm:ss` in range (a leap second is 60).
fn is_time(text: &str) -> bool {
    let parts: Vec<&str> = text.split(':').collect();
    let number = |part: &str, top: u32| {
        part.len() == 2
            && part.chars().all(|c| c.is_ascii_digit())
            && part.parse::<u32>().is_ok_and(|value| value <= top)
    };
    matches!(parts.as_slice(), [hours, minutes, seconds]
        if number(hours, 23) && number(minutes, 59) && number(seconds, 60))
}

/// The end of a date-time after the seconds: optional fractions, then `Z`
/// or an offset `+hh:mm` or `-hh:mm`.
fn is_offset(text: &str) -> bool {
    let rest = match text.strip_prefix('.') {
        Some(fraction) => {
            let rest = fraction.trim_start_matches(|c: char| c.is_ascii_digit());
            if rest.len() == fraction.len() {
                // A dot with no digit.
                return false;
            }
            rest
        }
        None => text,
    };
    rest.eq_ignore_ascii_case("z")
        || rest
            .strip_prefix(['+', '-'])
            .is_some_and(|offset| offset.len() == 5 && is_time(&format!("{offset}:00")))
}

/// A date as `YYYY-MM-DD` with a month and a day in range.
fn is_date(text: &str) -> bool {
    let parts: Vec<&str> = text.split('-').collect();
    let number = |part: &str, width: usize| {
        (part.len() == width && part.chars().all(|c| c.is_ascii_digit()))
            .then(|| part.parse::<u32>().ok())
            .flatten()
    };
    match parts.as_slice() {
        [year, month, day] => {
            number(year, 4).is_some()
                && number(month, 2).is_some_and(|month| (1..=12).contains(&month))
                && number(day, 2).is_some_and(|day| (1..=31).contains(&day))
        }
        _ => false,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub value: String,
    pub title: String,
}

/// The answer of the user to the whole request.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// The values of a form.
    Accept(Map<String, Value>),
    /// The user opened the URL: an accept with no content.
    Opened,
    Decline,
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RequestError {
    #[error("the request has no mode that ncp knows: {0}")]
    Mode(String),
    #[error("field {0:?} has a type that a form cannot ask: {1}")]
    FieldType(String, String),
    #[error("the request is not valid: {0}")]
    Shape(String),
}

fn text(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

/// Reads the parameters of `elicitation/create`.
///
/// # Errors
///
/// Returns an error for a mode or a field type that ncp does not know.
pub fn parse(params: &Value) -> Result<Request, RequestError> {
    let message = text(params, "message").unwrap_or_default();
    match params.get("mode").and_then(Value::as_str).unwrap_or("form") {
        "url" => Ok(Request::Url {
            message,
            url: text(params, "url").ok_or_else(|| RequestError::Shape("no url".into()))?,
        }),
        "form" => {
            let schema = params
                .get("requestedSchema")
                .ok_or_else(|| RequestError::Shape("no requestedSchema".into()))?;
            let required: Vec<&str> = schema
                .get("required")
                .and_then(Value::as_array)
                .map(|names| names.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            let fields = schema
                .get("properties")
                .and_then(Value::as_object)
                .map(|properties| {
                    properties
                        .iter()
                        .map(|(name, property)| {
                            field(name, property, required.contains(&name.as_str()))
                        })
                        .collect::<Result<Vec<_>, _>>()
                })
                .transpose()?
                .unwrap_or_default();
            Ok(Request::Form { message, fields })
        }
        other => Err(RequestError::Mode(other.to_string())),
    }
}

/// The choices of a list, or `None` when the list is empty or an item is
/// not a choice: a field with no choice cannot get an answer.
fn choices(list: &Value) -> Option<Vec<Choice>> {
    let items = list.as_array().filter(|items| !items.is_empty())?;
    items
        .iter()
        .map(|item| match item {
            Value::String(value) => Some(Choice {
                value: value.clone(),
                title: value.clone(),
            }),
            Value::Object(_) => {
                let value = text(item, "const")?;
                Some(Choice {
                    title: text(item, "title").unwrap_or_else(|| value.clone()),
                    value,
                })
            }
            _ => None,
        })
        .collect()
}

/// The choices of a single-select string: `enum` (with `enumNames` as the
/// titles), or `oneOf` with `const` and `title`.
fn single_choices(property: &Value) -> Option<Vec<Choice>> {
    if let Some(list) = property.get("oneOf") {
        return choices(list);
    }
    let mut found = choices(property.get("enum")?)?;
    if let Some(names) = property.get("enumNames").and_then(Value::as_array) {
        for (choice, name) in found.iter_mut().zip(names) {
            if let Some(name) = name.as_str() {
                choice.title = name.to_string();
            }
        }
    }
    Some(found)
}

fn field(name: &str, property: &Value, required: bool) -> Result<Field, RequestError> {
    let number = |key: &str| property.get(key).and_then(Value::as_f64);
    let kind = match property.get("type").and_then(Value::as_str) {
        // A list that is empty, or whose items do not all have a value,
        // is not a list that ncp can show; free text would send a value
        // that is not in it.
        Some("string") if property.get("oneOf").is_some() || property.get("enum").is_some() => {
            single_choices(property)
                .map(Kind::Choice)
                .ok_or_else(|| RequestError::FieldType(name.to_string(), "list".into()))?
        }
        Some("string") => match single_choices(property) {
            Some(list) => Kind::Choice(list),
            None => Kind::Text {
                min_length: property.get("minLength").and_then(Value::as_u64),
                max_length: property.get("maxLength").and_then(Value::as_u64),
                format: property
                    .get("format")
                    .and_then(Value::as_str)
                    .and_then(TextFormat::parse),
            },
        },
        Some("integer") => Kind::Integer {
            minimum: number("minimum"),
            maximum: number("maximum"),
        },
        Some("number") => Kind::Number {
            minimum: number("minimum"),
            maximum: number("maximum"),
        },
        Some("boolean") => Kind::Boolean,
        Some("array") => {
            let items = property.get("items").unwrap_or(&Value::Null);
            let list = items
                .get("anyOf")
                .and_then(choices)
                .or_else(|| items.get("enum").and_then(choices))
                .ok_or_else(|| RequestError::FieldType(name.to_string(), "array".into()))?;
            Kind::Choices {
                list,
                min_items: property.get("minItems").and_then(Value::as_u64),
                max_items: property.get("maxItems").and_then(Value::as_u64),
            }
        }
        other => {
            return Err(RequestError::FieldType(
                name.to_string(),
                other.unwrap_or("none").to_string(),
            ));
        }
    };
    Ok(Field {
        name: name.to_string(),
        label: text(property, "title").unwrap_or_else(|| name.to_string()),
        description: text(property, "description"),
        kind,
        required,
        default: property.get("default").cloned(),
    })
}

/// The text that tells the user what to type for the field.
#[must_use]
pub fn hint(field: &Field) -> String {
    let kind = match &field.kind {
        Kind::Text {
            format: Some(format),
            ..
        } => format.name().to_string(),
        Kind::Text { format: None, .. } => "text".to_string(),
        Kind::Integer { .. } => "whole number".to_string(),
        Kind::Number { .. } => "number".to_string(),
        Kind::Boolean => "yes or no".to_string(),
        Kind::Choice(list) => format!("one of: {}", titles(list)),
        Kind::Choices {
            list,
            min_items,
            max_items,
        } => match (min_items, max_items) {
            (None, None) => format!("some of, with commas: {}", titles(list)),
            (minimum, maximum) => format!(
                "{} of, with commas: {}",
                count_text(minimum.unwrap_or(0), *maximum),
                titles(list)
            ),
        },
    };
    let need = if field.required { ", required" } else { "" };
    let default = field
        .default
        .as_ref()
        .map(|value| format!(" [{}]", shown(value)))
        .unwrap_or_default();
    format!("{} ({kind}{need}){default}", field.label)
}

/// The count of values that a multi-select takes, as words.
fn count_text(minimum: u64, maximum: Option<u64>) -> String {
    match maximum {
        Some(maximum) if maximum == minimum => format!("{minimum}"),
        Some(maximum) => format!("{minimum} to {maximum}"),
        None => format!("at least {minimum}"),
    }
}

fn titles(list: &[Choice]) -> String {
    list.iter()
        .map(|choice| {
            if choice.title == choice.value {
                choice.value.clone()
            } else {
                format!("{} ({})", choice.value, choice.title)
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn shown(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Array(items) => items.iter().map(shown).collect::<Vec<_>>().join(","),
        other => other.to_string(),
    }
}

/// The value of an answer. An empty answer gives the default, or no value
/// for a field that is not required.
///
/// # Errors
///
/// Returns the text to show when the answer does not fit the field.
pub fn answer(field: &Field, typed: &str) -> Result<Option<Value>, String> {
    let typed = typed.trim();
    if typed.is_empty() {
        return match (&field.default, field.required) {
            (Some(default), _) => Ok(Some(default.clone())),
            (None, false) => Ok(None),
            (None, true) => Err("a value is necessary".into()),
        };
    }
    let in_range = |value: f64, minimum: Option<f64>, maximum: Option<f64>| {
        if minimum.is_some_and(|minimum| value < minimum)
            || maximum.is_some_and(|maximum| value > maximum)
        {
            Err(format!(
                "the value must be from {} to {}",
                minimum.map_or("any".into(), |m| m.to_string()),
                maximum.map_or("any".into(), |m| m.to_string())
            ))
        } else {
            Ok(())
        }
    };
    let pick = |list: &[Choice], word: &str| {
        list.iter()
            .find(|choice| choice.value == word || choice.title.eq_ignore_ascii_case(word))
            .map(|choice| json!(choice.value))
            .ok_or_else(|| format!("{word:?} is not one of: {}", titles(list)))
    };
    match &field.kind {
        Kind::Text {
            min_length,
            max_length,
            format,
        } => {
            if let Some(format) = format.filter(|format| !format.accepts(typed)) {
                return Err(format!("type a {}", format.name()));
            }
            let length = typed.chars().count() as u64;
            if min_length.is_some_and(|min| length < min)
                || max_length.is_some_and(|max| length > max)
            {
                return Err(format!(
                    "the text must have {} to {} characters",
                    min_length.unwrap_or(0),
                    max_length.map_or("any".into(), |max| max.to_string())
                ));
            }
            Ok(Some(json!(typed)))
        }
        Kind::Integer { minimum, maximum } => {
            let value: i64 = typed
                .parse()
                .map_err(|_| "type a whole number".to_string())?;
            #[allow(
                clippy::cast_precision_loss,
                reason = "a range check of a typed number"
            )]
            in_range(value as f64, *minimum, *maximum)?;
            Ok(Some(json!(value)))
        }
        Kind::Number { minimum, maximum } => {
            let value: f64 = typed
                .parse()
                .ok()
                .filter(|value: &f64| value.is_finite())
                .ok_or_else(|| "type a number".to_string())?;
            in_range(value, *minimum, *maximum)?;
            Ok(Some(json!(value)))
        }
        Kind::Boolean => match typed.to_ascii_lowercase().as_str() {
            "y" | "yes" | "true" => Ok(Some(json!(true))),
            "n" | "no" | "false" => Ok(Some(json!(false))),
            _ => Err("type yes or no".into()),
        },
        Kind::Choice(list) => pick(list, typed).map(Some),
        Kind::Choices {
            list,
            min_items,
            max_items,
        } => {
            let values = typed
                .split(',')
                .map(str::trim)
                .filter(|word| !word.is_empty())
                .map(|word| pick(list, word))
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .fold(Vec::new(), |mut values, value| {
                    if !values.contains(&value) {
                        values.push(value);
                    }
                    values
                });
            let count = values.len() as u64;
            let minimum = min_items.unwrap_or(u64::from(field.required));
            if count < minimum || max_items.is_some_and(|max| count > max) {
                return Err(format!("choose {}", count_text(minimum, *max_items)));
            }
            Ok(Some(Value::Array(values)))
        }
    }
}

/// The `ElicitResult` of the action.
#[must_use]
pub fn result(action: &Action) -> Value {
    match action {
        Action::Accept(content) => json!({"action": "accept", "content": content}),
        Action::Opened => json!({"action": "accept"}),
        Action::Decline => json!({"action": "decline"}),
        Action::Cancel => json!({"action": "cancel"}),
    }
}

/// The URL of a URL-mode request when it is safe to open: `https`, or
/// `http` to a loopback host. Another scheme (`file:`, `smb:`, a custom
/// one) starts a program, not a page.
///
/// # Errors
///
/// Returns the reason when the URL is not safe to open.
pub fn safe_url(text: &str) -> Result<url::Url, String> {
    let url = url::Url::parse(text).map_err(|error| format!("the URL is not valid: {error}"))?;
    if crate::server::is_secure_url(&url) {
        Ok(url)
    } else {
        Err(format!("ncp opens only https pages, not {}:", url.scheme()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form() -> Value {
        json!({
            "message": "Who are you?",
            "requestedSchema": {"type": "object", "properties": {
                "name": {"type": "string", "title": "Name", "minLength": 1},
                "age": {"type": "integer", "minimum": 0, "maximum": 150, "default": 30},
                "subscribe": {"type": "boolean"},
                "color": {"type": "string", "enum": ["r", "g"], "enumNames": ["Red", "Green"]},
                "size": {"type": "string", "oneOf": [{"const": "s", "title": "Small"}]},
                "tags": {"type": "array", "items": {"enum": ["a", "b"]}}
            }, "required": ["name"]}
        })
    }

    #[test]
    fn reads_a_form_with_each_primitive_type() {
        let Request::Form { message, fields } = parse(&form()).unwrap() else {
            panic!("not a form")
        };
        assert_eq!(message, "Who are you?");
        let field = |name: &str| {
            fields
                .iter()
                .find(|field| field.name == name)
                .unwrap()
                .clone()
        };
        assert!(field("name").required && !field("age").required);
        assert_eq!(field("name").label, "Name");
        assert_eq!(field("age").default, Some(json!(30)));
        assert!(matches!(field("subscribe").kind, Kind::Boolean));
        assert_eq!(
            field("color").kind,
            Kind::Choice(vec![
                Choice {
                    value: "r".into(),
                    title: "Red".into()
                },
                Choice {
                    value: "g".into(),
                    title: "Green".into()
                },
            ])
        );
        assert!(matches!(field("size").kind, Kind::Choice(_)));
        assert!(matches!(field("tags").kind, Kind::Choices { .. }));
    }

    fn one_field(property: &Value, required: bool) -> Field {
        field("f", property, required).unwrap()
    }

    #[test]
    fn text_formats_catch_typing_errors() {
        let check = |format: &str, typed: &str| {
            answer(
                &one_field(&json!({"type": "string", "format": format}), false),
                typed,
            )
            .is_ok()
        };
        assert!(check("email", "ada@example.test") && !check("email", "ada"));
        assert!(check("uri", "https://a.test/x") && !check("uri", "a test"));
        assert!(check("date", "2026-10-09") && !check("date", "2026-13-01"));
        assert!(!check("date", "9/10/2026"));
        assert!(check("date-time", "2026-10-09T10:00:00Z"));
        assert!(check("date-time", "2026-10-09T10:00:00+11:00"));
        assert!(!check("date-time", "2026-10-09") && !check("date-time", "2026-10-09T10:00"));
        assert!(check("date-time", "2026-10-09t10:00:00.5z"));
        assert!(!check("date-time", "2026-10-09TZ") && !check("date-time", "2026-10-09Tx-"));
        assert!(!check("date-time", "2026-10-09T10:00:00.Z"));
        assert!(check("email", "root@localhost") && !check("email", "@x"));
        // An unknown format is free text.
        assert!(check("color", "anything"));
    }

    #[test]
    fn multi_select_checks_the_count_of_values() {
        let tags = json!({"type": "array", "items": {"enum": ["a", "b", "c"]},
            "minItems": 1, "maxItems": 2});
        assert!(answer(&one_field(&tags, false), "a, b").is_ok());
        assert!(answer(&one_field(&tags, false), "a, b, c").is_err());
        assert!(answer(&one_field(&tags, false), " , ").is_err());
        let free = json!({"type": "array", "items": {"enum": ["a"]}});
        assert!(answer(&one_field(&free, true), " , ").is_err());
        assert_eq!(answer(&one_field(&free, false), " , "), Ok(Some(json!([]))));
        assert_eq!(
            answer(&one_field(&tags, false), "a, a"),
            Ok(Some(json!(["a"])))
        );
        assert!(hint(&one_field(&tags, false)).contains("1 to 2 of, with commas"));
        let empty = json!({"type": "string", "enum": []});
        assert!(matches!(
            field("f", &empty, true),
            Err(RequestError::FieldType(..))
        ));
    }

    #[test]
    fn one_of_with_an_item_with_no_const_is_refused() {
        let property = json!({"type": "string", "oneOf": [{"const": "a"}, {"title": "B"}]});
        assert!(matches!(
            field("f", &property, false),
            Err(RequestError::FieldType(..))
        ));
    }

    #[test]
    fn reads_a_url_request_and_refuses_an_unknown_mode_or_type() {
        assert_eq!(
            parse(&json!({"mode": "url", "message": "m", "url": "https://a.test/x"})).unwrap(),
            Request::Url {
                message: "m".into(),
                url: "https://a.test/x".into()
            }
        );
        assert!(matches!(
            parse(&json!({"mode": "x"})),
            Err(RequestError::Mode(_))
        ));
        let nested =
            json!({"message": "m", "requestedSchema": {"properties": {"o": {"type": "object"}}}});
        assert!(matches!(parse(&nested), Err(RequestError::FieldType(..))));
    }

    #[test]
    fn answers_are_checked_and_empty_takes_the_default() {
        let Request::Form { fields, .. } = parse(&form()).unwrap() else {
            panic!("not a form")
        };
        let field = |name: &str| {
            fields
                .iter()
                .find(|field| field.name == name)
                .unwrap()
                .clone()
        };
        assert_eq!(answer(&field("age"), ""), Ok(Some(json!(30))));
        assert_eq!(answer(&field("age"), "41"), Ok(Some(json!(41))));
        assert!(answer(&field("age"), "200").is_err());
        assert!(answer(&field("age"), "x").is_err());
        let number = Field {
            kind: Kind::Number {
                minimum: None,
                maximum: None,
            },
            ..field("age")
        };
        assert!(answer(&number, "nan").is_err() && answer(&number, "inf").is_err());
        assert!(answer(&field("name"), "").is_err());
        assert_eq!(answer(&field("subscribe"), ""), Ok(None));
        assert_eq!(answer(&field("subscribe"), "yes"), Ok(Some(json!(true))));
        assert_eq!(answer(&field("color"), "green"), Ok(Some(json!("g"))));
        assert!(answer(&field("color"), "blue").is_err());
        assert_eq!(answer(&field("tags"), "a, b"), Ok(Some(json!(["a", "b"]))));
    }

    #[test]
    fn results_have_the_action_and_the_content_of_an_accept_only() {
        let content: Map<String, Value> =
            [("name".to_string(), json!("Ada"))].into_iter().collect();
        assert_eq!(
            result(&Action::Accept(content)),
            json!({"action": "accept", "content": {"name": "Ada"}})
        );
        assert_eq!(result(&Action::Decline), json!({"action": "decline"}));
        assert_eq!(result(&Action::Cancel), json!({"action": "cancel"}));
        assert_eq!(result(&Action::Opened), json!({"action": "accept"}));
        assert_eq!(
            result(&Action::Accept(Map::new())),
            json!({"action": "accept", "content": {}})
        );
    }

    #[test]
    fn pages_come_from_the_data_of_the_error() {
        let data = json!({"elicitations": [
            {"mode": "url", "elicitationId": "e", "url": "https://A.test/x", "message": "Sign in"},
            {"mode": "url", "message": "no url"},
            {"mode": "url", "url": "javascript:alert(1)"}
        ]});
        let found = pages(Some(&data));
        assert_eq!(
            found[0],
            Ok(Page {
                message: "Sign in".into(),
                url: "https://a.test/x".into()
            })
        );
        assert!(found[1].is_err() && found.len() == 2);
        assert_eq!(pages(None), []);
    }

    #[test]
    fn only_https_or_loopback_http_urls_open() {
        assert!(safe_url("https://a.test/x").is_ok());
        assert!(safe_url("http://127.0.0.1:8/x").is_ok());
        assert!(safe_url("http://a.test/x").is_err());
        assert!(safe_url("file:///Applications/Calculator.app").is_err());
        assert!(safe_url("-aCalculator").is_err());
        assert!(safe_url("smb://host/share").is_err());
    }
}
