//! Transcripts of the completion scripts, the hidden completion commands
//! and the root help.
//!
//! The hidden `__complete-*` commands have no `--output-format`: the shell
//! reads their plain lines, so they have only text transcripts.

#![allow(clippy::unwrap_used)]
// One inline snapshot holds a whole completion script or help text.
#![allow(clippy::too_many_lines)]

mod common;

use common::{Home, ncp_snapshot};
use serde_json::json;

const FIXTURE: &str = env!("CARGO_BIN_EXE_ncp-fixture");

#[test]
fn completion_script_for_bash() {
    let home = Home::new("completion-snapshot-bash");
    ncp_snapshot!(home.filters(), home.ncp(["generate-shell-completion", "bash"]), @r#"
    exit_code: 0 (success)
    ----- stdout -----
    _ncp() {
        local i cur prev opts cmd
        COMPREPLY=()
        if [[ "${BASH_VERSINFO[0]}" -ge 4 ]]; then
            cur="$2"
        else
            cur="${COMP_WORDS[COMP_CWORD]}"
        fi
        prev="$3"
        cmd=""
        opts=""

        for i in "${COMP_WORDS[@]:0:COMP_CWORD}"
        do
            case "${cmd},${i}" in
                ",$1")
                    cmd="ncp"
                    ;;
                ncp,auth)
                    cmd="ncp__subcmd__auth"
                    ;;
                ncp,generate-shell-completion)
                    cmd="ncp__subcmd__generate__subcmd__shell__subcmd__completion"
                    ;;
                ncp,prompt)
                    cmd="ncp__subcmd__prompt"
                    ;;
                ncp,resource)
                    cmd="ncp__subcmd__resource"
                    ;;
                ncp,secret)
                    cmd="ncp__subcmd__secret"
                    ;;
                ncp,server)
                    cmd="ncp__subcmd__server"
                    ;;
                ncp,tool)
                    cmd="ncp__subcmd__tool"
                    ;;
                ncp,trust)
                    cmd="ncp__subcmd__trust"
                    ;;
                ncp__subcmd__auth,login)
                    cmd="ncp__subcmd__auth__subcmd__login"
                    ;;
                ncp__subcmd__auth,logout)
                    cmd="ncp__subcmd__auth__subcmd__logout"
                    ;;
                ncp__subcmd__auth,status)
                    cmd="ncp__subcmd__auth__subcmd__status"
                    ;;
                ncp__subcmd__prompt,get)
                    cmd="ncp__subcmd__prompt__subcmd__get"
                    ;;
                ncp__subcmd__prompt,list)
                    cmd="ncp__subcmd__prompt__subcmd__list"
                    ;;
                ncp__subcmd__resource,list)
                    cmd="ncp__subcmd__resource__subcmd__list"
                    ;;
                ncp__subcmd__resource,read)
                    cmd="ncp__subcmd__resource__subcmd__read"
                    ;;
                ncp__subcmd__secret,delete)
                    cmd="ncp__subcmd__secret__subcmd__delete"
                    ;;
                ncp__subcmd__secret,list)
                    cmd="ncp__subcmd__secret__subcmd__list"
                    ;;
                ncp__subcmd__secret,set)
                    cmd="ncp__subcmd__secret__subcmd__set"
                    ;;
                ncp__subcmd__server,add)
                    cmd="ncp__subcmd__server__subcmd__add"
                    ;;
                ncp__subcmd__server,list)
                    cmd="ncp__subcmd__server__subcmd__list"
                    ;;
                ncp__subcmd__server,remove)
                    cmd="ncp__subcmd__server__subcmd__remove"
                    ;;
                ncp__subcmd__server,show)
                    cmd="ncp__subcmd__server__subcmd__show"
                    ;;
                ncp__subcmd__tool,call)
                    cmd="ncp__subcmd__tool__subcmd__call"
                    ;;
                ncp__subcmd__tool,list)
                    cmd="ncp__subcmd__tool__subcmd__list"
                    ;;
                ncp__subcmd__tool,show)
                    cmd="ncp__subcmd__tool__subcmd__show"
                    ;;
                *)
                    ;;
            esac
        done

        case "${cmd}" in
            ncp)
                opts="-v -q -h -V --timeout --verbose --timings --quiet --no-cache --refresh --output-format --config-file --help --version server tool resource prompt auth secret trust generate-shell-completion"
                if [[ ${cur} == -* || ${COMP_CWORD} -eq 1 ]] ; then
                    COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                    return 0
                fi
                case "${prev}" in
                    --timeout)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    --output-format)
                        COMPREPLY=($(compgen -W "text json" -- "${cur}"))
                        return 0
                        ;;
                    --config-file)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    *)
                        COMPREPLY=()
                        ;;
                esac
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
                ;;
            ncp__subcmd__auth)
                opts="-v -q -h -V --timeout --verbose --timings --quiet --no-cache --refresh --output-format --config-file --help --version login logout status"
                if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                    COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                    return 0
                fi
                case "${prev}" in
                    --timeout)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    --output-format)
                        COMPREPLY=($(compgen -W "text json" -- "${cur}"))
                        return 0
                        ;;
                    --config-file)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    *)
                        COMPREPLY=()
                        ;;
                esac
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
                ;;
            ncp__subcmd__auth__subcmd__login)
                opts="$(ncp __complete-servers 2>/dev/null) -v -q -h -V --no-browser --credential-store --timeout --verbose --timings --quiet --no-cache --refresh --output-format --config-file --help --version"
                if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                    COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                    return 0
                fi
                case "${prev}" in
                    --credential-store)
                        COMPREPLY=($(compgen -W "native file" -- "${cur}"))
                        return 0
                        ;;
                    --timeout)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    --output-format)
                        COMPREPLY=($(compgen -W "text json" -- "${cur}"))
                        return 0
                        ;;
                    --config-file)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    *)
                        COMPREPLY=()
                        ;;
                esac
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
                ;;
            ncp__subcmd__auth__subcmd__logout)
                opts="$(ncp __complete-servers 2>/dev/null) -v -q -h -V --timeout --verbose --timings --quiet --no-cache --refresh --output-format --config-file --help --version"
                if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                    COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                    return 0
                fi
                case "${prev}" in
                    --timeout)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    --output-format)
                        COMPREPLY=($(compgen -W "text json" -- "${cur}"))
                        return 0
                        ;;
                    --config-file)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    *)
                        COMPREPLY=()
                        ;;
                esac
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
                ;;
            ncp__subcmd__auth__subcmd__status)
                opts="$(ncp __complete-servers 2>/dev/null) -v -q -h -V --timeout --verbose --timings --quiet --no-cache --refresh --output-format --config-file --help --version"
                if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                    COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                    return 0
                fi
                case "${prev}" in
                    --timeout)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    --output-format)
                        COMPREPLY=($(compgen -W "text json" -- "${cur}"))
                        return 0
                        ;;
                    --config-file)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    *)
                        COMPREPLY=()
                        ;;
                esac
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
                ;;
            ncp__subcmd__generate__subcmd__shell__subcmd__completion)
                opts="-v -q -h -V --timeout --verbose --timings --quiet --no-cache --refresh --output-format --config-file --help --version bash zsh fish"
                if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                    COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                    return 0
                fi
                case "${prev}" in
                    --timeout)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    --output-format)
                        COMPREPLY=($(compgen -W "text json" -- "${cur}"))
                        return 0
                        ;;
                    --config-file)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    *)
                        COMPREPLY=()
                        ;;
                esac
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
                ;;
            ncp__subcmd__prompt)
                opts="-v -q -h -V --timeout --verbose --timings --quiet --no-cache --refresh --output-format --config-file --help --version list get"
                if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                    COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                    return 0
                fi
                case "${prev}" in
                    --timeout)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    --output-format)
                        COMPREPLY=($(compgen -W "text json" -- "${cur}"))
                        return 0
                        ;;
                    --config-file)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    *)
                        COMPREPLY=()
                        ;;
                esac
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
                ;;
            ncp__subcmd__prompt__subcmd__get)
                opts="$(ncp __complete-servers 2>/dev/null) -v -q -h -V --timeout --verbose --timings --quiet --no-cache --refresh --output-format --config-file --help --version"
                if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                    COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                    return 0
                fi
                case "${prev}" in
                    --timeout)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    --output-format)
                        COMPREPLY=($(compgen -W "text json" -- "${cur}"))
                        return 0
                        ;;
                    --config-file)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    *)
                        COMPREPLY=()
                        ;;
                esac
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
                ;;
            ncp__subcmd__prompt__subcmd__list)
                opts="$(ncp __complete-servers 2>/dev/null) -v -q -h -V --timeout --verbose --timings --quiet --no-cache --refresh --output-format --config-file --help --version"
                if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                    COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                    return 0
                fi
                case "${prev}" in
                    --timeout)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    --output-format)
                        COMPREPLY=($(compgen -W "text json" -- "${cur}"))
                        return 0
                        ;;
                    --config-file)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    *)
                        COMPREPLY=()
                        ;;
                esac
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
                ;;
            ncp__subcmd__resource)
                opts="-v -q -h -V --timeout --verbose --timings --quiet --no-cache --refresh --output-format --config-file --help --version list read"
                if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                    COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                    return 0
                fi
                case "${prev}" in
                    --timeout)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    --output-format)
                        COMPREPLY=($(compgen -W "text json" -- "${cur}"))
                        return 0
                        ;;
                    --config-file)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    *)
                        COMPREPLY=()
                        ;;
                esac
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
                ;;
            ncp__subcmd__resource__subcmd__list)
                opts="$(ncp __complete-servers 2>/dev/null) -v -q -h -V --timeout --verbose --timings --quiet --no-cache --refresh --output-format --config-file --help --version"
                if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                    COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                    return 0
                fi
                case "${prev}" in
                    --timeout)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    --output-format)
                        COMPREPLY=($(compgen -W "text json" -- "${cur}"))
                        return 0
                        ;;
                    --config-file)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    *)
                        COMPREPLY=()
                        ;;
                esac
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
                ;;
            ncp__subcmd__resource__subcmd__read)
                opts="$(ncp __complete-servers 2>/dev/null) -v -q -h -V --timeout --verbose --timings --quiet --no-cache --refresh --output-format --config-file --help --version"
                if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                    COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                    return 0
                fi
                case "${prev}" in
                    --timeout)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    --output-format)
                        COMPREPLY=($(compgen -W "text json" -- "${cur}"))
                        return 0
                        ;;
                    --config-file)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    *)
                        COMPREPLY=()
                        ;;
                esac
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
                ;;
            ncp__subcmd__secret)
                opts="-v -q -h -V --timeout --verbose --timings --quiet --no-cache --refresh --output-format --config-file --help --version set list delete"
                if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                    COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                    return 0
                fi
                case "${prev}" in
                    --timeout)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    --output-format)
                        COMPREPLY=($(compgen -W "text json" -- "${cur}"))
                        return 0
                        ;;
                    --config-file)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    *)
                        COMPREPLY=()
                        ;;
                esac
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
                ;;
            ncp__subcmd__secret__subcmd__delete)
                opts="-v -q -h -V --server --unused --all --timeout --verbose --timings --quiet --no-cache --refresh --output-format --config-file --help --version"
                if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                    COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                    return 0
                fi
                case "${prev}" in
                    --server)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    --timeout)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    --output-format)
                        COMPREPLY=($(compgen -W "text json" -- "${cur}"))
                        return 0
                        ;;
                    --config-file)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    *)
                        COMPREPLY=()
                        ;;
                esac
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
                ;;
            ncp__subcmd__secret__subcmd__list)
                opts="-v -q -h -V --timeout --verbose --timings --quiet --no-cache --refresh --output-format --config-file --help --version"
                if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                    COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                    return 0
                fi
                case "${prev}" in
                    --timeout)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    --output-format)
                        COMPREPLY=($(compgen -W "text json" -- "${cur}"))
                        return 0
                        ;;
                    --config-file)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    *)
                        COMPREPLY=()
                        ;;
                esac
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
                ;;
            ncp__subcmd__secret__subcmd__set)
                opts="-v -q -h -V --server --credential-store --timeout --verbose --timings --quiet --no-cache --refresh --output-format --config-file --help --version"
                if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                    COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                    return 0
                fi
                case "${prev}" in
                    --server)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    --credential-store)
                        COMPREPLY=($(compgen -W "native file" -- "${cur}"))
                        return 0
                        ;;
                    --timeout)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    --output-format)
                        COMPREPLY=($(compgen -W "text json" -- "${cur}"))
                        return 0
                        ;;
                    --config-file)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    *)
                        COMPREPLY=()
                        ;;
                esac
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
                ;;
            ncp__subcmd__server)
                opts="-v -q -h -V --timeout --verbose --timings --quiet --no-cache --refresh --output-format --config-file --help --version add remove list show"
                if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                    COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                    return 0
                fi
                case "${prev}" in
                    --timeout)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    --output-format)
                        COMPREPLY=($(compgen -W "text json" -- "${cur}"))
                        return 0
                        ;;
                    --config-file)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    *)
                        COMPREPLY=()
                        ;;
                esac
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
                ;;
            ncp__subcmd__server__subcmd__add)
                opts="-v -q -h -V --global --header --env --secret --credential-store --timeout --verbose --timings --quiet --no-cache --refresh --output-format --config-file --help --version"
                if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                    COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                    return 0
                fi
                case "${prev}" in
                    --header)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    --env)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    --secret)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    --credential-store)
                        COMPREPLY=($(compgen -W "native file" -- "${cur}"))
                        return 0
                        ;;
                    --timeout)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    --output-format)
                        COMPREPLY=($(compgen -W "text json" -- "${cur}"))
                        return 0
                        ;;
                    --config-file)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    *)
                        COMPREPLY=()
                        ;;
                esac
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
                ;;
            ncp__subcmd__server__subcmd__list)
                opts="-v -q -h -V --timeout --verbose --timings --quiet --no-cache --refresh --output-format --config-file --help --version"
                if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                    COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                    return 0
                fi
                case "${prev}" in
                    --timeout)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    --output-format)
                        COMPREPLY=($(compgen -W "text json" -- "${cur}"))
                        return 0
                        ;;
                    --config-file)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    *)
                        COMPREPLY=()
                        ;;
                esac
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
                ;;
            ncp__subcmd__server__subcmd__remove)
                opts="$(ncp __complete-servers 2>/dev/null) -v -q -h -V --global --timeout --verbose --timings --quiet --no-cache --refresh --output-format --config-file --help --version"
                if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                    COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                    return 0
                fi
                case "${prev}" in
                    --timeout)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    --output-format)
                        COMPREPLY=($(compgen -W "text json" -- "${cur}"))
                        return 0
                        ;;
                    --config-file)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    *)
                        COMPREPLY=()
                        ;;
                esac
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
                ;;
            ncp__subcmd__server__subcmd__show)
                opts="$(ncp __complete-servers 2>/dev/null) -v -q -h -V --timeout --verbose --timings --quiet --no-cache --refresh --output-format --config-file --help --version"
                if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                    COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                    return 0
                fi
                case "${prev}" in
                    --timeout)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    --output-format)
                        COMPREPLY=($(compgen -W "text json" -- "${cur}"))
                        return 0
                        ;;
                    --config-file)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    *)
                        COMPREPLY=()
                        ;;
                esac
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
                ;;
            ncp__subcmd__tool)
                opts="-v -q -h -V --timeout --verbose --timings --quiet --no-cache --refresh --output-format --config-file --help --version list show call"
                if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                    COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                    return 0
                fi
                case "${prev}" in
                    --timeout)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    --output-format)
                        COMPREPLY=($(compgen -W "text json" -- "${cur}"))
                        return 0
                        ;;
                    --config-file)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    *)
                        COMPREPLY=()
                        ;;
                esac
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
                ;;
            ncp__subcmd__tool__subcmd__call)
                opts="$(ncp __complete-servers 2>/dev/null) -h -v -q -V --no-validate --help --timeout --verbose --timings --quiet --no-cache --refresh --output-format --config-file --version"
                if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                    COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                    return 0
                fi
                if [[ ${cur} != -* ]] ; then
                    local ncp_words=() ncp_skip=0 ncp_word
                    for ncp_word in "${COMP_WORDS[@]:1:COMP_CWORD-1}"; do
                        if [[ ${ncp_skip} -eq 1 ]] ; then ncp_skip=0 ; continue ; fi
                        case "${ncp_word}" in
                            --timeout|--output-format|--config-file) ncp_skip=1 ;;
                            -*) ;;
                            *) ncp_words+=("${ncp_word}") ;;
                        esac
                    done
                    if [[ ${ncp_skip} -eq 0 && ${#ncp_words[@]} -eq 3 ]] ; then
                        COMPREPLY=( $(compgen -W "$(ncp __complete-tools "${ncp_words[2]}" 2>/dev/null)" -- "${cur}") )
                        return 0
                    fi
                fi
                case "${prev}" in
                    --timeout)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    --output-format)
                        COMPREPLY=($(compgen -W "text json" -- "${cur}"))
                        return 0
                        ;;
                    --config-file)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    *)
                        COMPREPLY=()
                        ;;
                esac
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
                ;;
            ncp__subcmd__tool__subcmd__list)
                opts="$(ncp __complete-servers 2>/dev/null) -v -q -h -V --timeout --verbose --timings --quiet --no-cache --refresh --output-format --config-file --help --version"
                if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                    COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                    return 0
                fi
                case "${prev}" in
                    --timeout)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    --output-format)
                        COMPREPLY=($(compgen -W "text json" -- "${cur}"))
                        return 0
                        ;;
                    --config-file)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    *)
                        COMPREPLY=()
                        ;;
                esac
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
                ;;
            ncp__subcmd__tool__subcmd__show)
                opts="$(ncp __complete-servers 2>/dev/null) -v -q -h -V --timeout --verbose --timings --quiet --no-cache --refresh --output-format --config-file --help --version"
                if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                    COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                    return 0
                fi
                if [[ ${cur} != -* ]] ; then
                    local ncp_words=() ncp_skip=0 ncp_word
                    for ncp_word in "${COMP_WORDS[@]:1:COMP_CWORD-1}"; do
                        if [[ ${ncp_skip} -eq 1 ]] ; then ncp_skip=0 ; continue ; fi
                        case "${ncp_word}" in
                            --timeout|--output-format|--config-file) ncp_skip=1 ;;
                            -*) ;;
                            *) ncp_words+=("${ncp_word}") ;;
                        esac
                    done
                    if [[ ${ncp_skip} -eq 0 && ${#ncp_words[@]} -eq 3 ]] ; then
                        COMPREPLY=( $(compgen -W "$(ncp __complete-tools "${ncp_words[2]}" 2>/dev/null)" -- "${cur}") )
                        return 0
                    fi
                fi
                case "${prev}" in
                    --timeout)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    --output-format)
                        COMPREPLY=($(compgen -W "text json" -- "${cur}"))
                        return 0
                        ;;
                    --config-file)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    *)
                        COMPREPLY=()
                        ;;
                esac
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
                ;;
            ncp__subcmd__trust)
                opts="-v -q -h -V --show --untrust --ignore --timeout --verbose --timings --quiet --no-cache --refresh --output-format --config-file --help --version"
                if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                    COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                    return 0
                fi
                case "${prev}" in
                    --timeout)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    --output-format)
                        COMPREPLY=($(compgen -W "text json" -- "${cur}"))
                        return 0
                        ;;
                    --config-file)
                        COMPREPLY=($(compgen -f "${cur}"))
                        return 0
                        ;;
                    *)
                        COMPREPLY=()
                        ;;
                esac
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
                ;;
        esac
    }

    if [[ "${BASH_VERSINFO[0]}" -eq 4 && "${BASH_VERSINFO[1]}" -ge 4 || "${BASH_VERSINFO[0]}" -gt 4 ]]; then
        complete -F _ncp -o nosort -o bashdefault -o default ncp
    else
        complete -F _ncp -o bashdefault -o default ncp
    fi
    "#);
}

#[test]
fn completion_script_for_zsh() {
    let home = Home::new("completion-snapshot-zsh");
    ncp_snapshot!(home.filters(), home.ncp(["generate-shell-completion", "zsh"]), @r#"
    exit_code: 0 (success)
    ----- stdout -----
    #compdef ncp

    _ncp_servers() {
        local -a servers
        servers=(${(f)"$(ncp __complete-servers 2>/dev/null)"})
        _describe 'server' servers
    }

    _ncp_tools() {
        local -a tools
        tools=(${(f)"$(ncp __complete-tools "${line[1]}" 2>/dev/null)"})
        _describe 'tool' tools
    }


    autoload -U is-at-least

    _ncp() {
        typeset -A opt_args
        typeset -a _arguments_options
        local ret=1

        if is-at-least 5.2; then
            _arguments_options=(-s -S -C)
        else
            _arguments_options=(-s -C)
        fi

        local context curcontext="$curcontext" state line
        _arguments "${_arguments_options[@]}" : \
    '--timeout=[The time limit in whole seconds for one request, a login or an input. After a timeout, ncp cancels the request or stops the connection]:SECONDS:_default' \
    '--output-format=[The output format. Default\: text]:FORMAT:((text\:"Text for people"
    json\:"One JSON value on stdout, and JSON Lines on stderr"))' \
    '--config-file=[Use this configuration file in place of the global file\: ncp/mcp.json in the XDG configuration directory, by default ~/.config/ncp/mcp.json. The file must exist. ncp then does not search for a project .mcp.json, and runs the servers of this file with no trust question]:PATH:_files' \
    '*-v[Show log lines on stderr\: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change]' \
    '*--verbose[Show log lines on stderr\: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change]' \
    '--timings[Show the time of each step on stderr\: CPU, configuration, credentials, server start, DNS, connect and each response]' \
    '(-v --verbose)*-q[Show no hints, no warnings and no progress on stderr. ncp still shows errors]' \
    '(-v --verbose)*--quiet[Show no hints, no warnings and no progress on stderr. ncp still shows errors]' \
    '--no-cache[Do not read or write the cache\: the protocol version and the tool list of each server]' \
    '--refresh[Do not read the cache, but write it\: ncp finds the protocol version and gets the tool list again. --no-cache has priority over --refresh]' \
    '-h[Print help (see more with '\''--help'\'')]' \
    '--help[Print help (see more with '\''--help'\'')]' \
    '-V[Print version]' \
    '--version[Print version]' \
    ":: :_ncp_commands" \
    "*::: :->ncp" \
    && ret=0
        case $state in
        (ncp)
            words=($line[1] "${words[@]}")
            (( CURRENT += 1 ))
            curcontext="${curcontext%:*:*}:ncp-command-$line[1]:"
            case $line[1] in
                (server)
    _arguments "${_arguments_options[@]}" : \
    '--timeout=[The time limit in whole seconds for one request, a login or an input. After a timeout, ncp cancels the request or stops the connection]:SECONDS:_default' \
    '--output-format=[The output format. Default\: text]:FORMAT:((text\:"Text for people"
    json\:"One JSON value on stdout, and JSON Lines on stderr"))' \
    '--config-file=[Use this configuration file in place of the global file\: ncp/mcp.json in the XDG configuration directory, by default ~/.config/ncp/mcp.json. The file must exist. ncp then does not search for a project .mcp.json, and runs the servers of this file with no trust question]:PATH:_files' \
    '*-v[Show log lines on stderr\: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change]' \
    '*--verbose[Show log lines on stderr\: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change]' \
    '--timings[Show the time of each step on stderr\: CPU, configuration, credentials, server start, DNS, connect and each response]' \
    '(-v --verbose)*-q[Show no hints, no warnings and no progress on stderr. ncp still shows errors]' \
    '(-v --verbose)*--quiet[Show no hints, no warnings and no progress on stderr. ncp still shows errors]' \
    '--no-cache[Do not read or write the cache\: the protocol version and the tool list of each server]' \
    '--refresh[Do not read the cache, but write it\: ncp finds the protocol version and gets the tool list again. --no-cache has priority over --refresh]' \
    '-h[Print help (see more with '\''--help'\'')]' \
    '--help[Print help (see more with '\''--help'\'')]' \
    '-V[Print version]' \
    '--version[Print version]' \
    ":: :_ncp__subcmd__server_commands" \
    "*::: :->server" \
    && ret=0

        case $state in
        (server)
            words=($line[1] "${words[@]}")
            (( CURRENT += 1 ))
            curcontext="${curcontext%:*:*}:ncp-server-command-$line[1]:"
            case $line[1] in
                (add)
    _arguments "${_arguments_options[@]}" : \
    '*--header=[A header of an HTTP server. The value of an Authorization or Proxy-Authorization header must be '\''<scheme> \${NAME}'\'' or '\''\${NAME}'\'']:'NAME: VALUE':_default' \
    '*--env=[An environment variable of a stdio server\: NAME writes \${NAME}, and NAME=VALUE writes VALUE with no change]:NAME[=VALUE]:_default' \
    '--secret=[Read the value of \${NAME} from the terminal or stdin, and store it for the destination of the entry]:NAME:_default' \
    '--credential-store=[The credential store for the value of --secret. Default\: native]:CREDENTIAL_STORE:((native\:"The native store\: the macOS Keychain or the Linux Secret Service"
    file\:"The credential file, with mode 0600"))' \
    '--timeout=[The time limit in whole seconds for one request, a login or an input. After a timeout, ncp cancels the request or stops the connection]:SECONDS:_default' \
    '--output-format=[The output format. Default\: text]:FORMAT:((text\:"Text for people"
    json\:"One JSON value on stdout, and JSON Lines on stderr"))' \
    '--config-file=[Use this configuration file in place of the global file\: ncp/mcp.json in the XDG configuration directory, by default ~/.config/ncp/mcp.json. The file must exist. ncp then does not search for a project .mcp.json, and runs the servers of this file with no trust question]:PATH:_files' \
    '--global[Add the entry to the global file]' \
    '*-v[Show log lines on stderr\: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change]' \
    '*--verbose[Show log lines on stderr\: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change]' \
    '--timings[Show the time of each step on stderr\: CPU, configuration, credentials, server start, DNS, connect and each response]' \
    '(-v --verbose)*-q[Show no hints, no warnings and no progress on stderr. ncp still shows errors]' \
    '(-v --verbose)*--quiet[Show no hints, no warnings and no progress on stderr. ncp still shows errors]' \
    '--no-cache[Do not read or write the cache\: the protocol version and the tool list of each server]' \
    '--refresh[Do not read the cache, but write it\: ncp finds the protocol version and gets the tool list again. --no-cache has priority over --refresh]' \
    '-h[Print help (see more with '\''--help'\'')]' \
    '--help[Print help (see more with '\''--help'\'')]' \
    '-V[Print version]' \
    '--version[Print version]' \
    ':server -- The server name\: letters, digits, - and _:_default' \
    '::url -- The URL of an HTTP server:_default' \
    '*::command -- The command of a stdio server and its arguments, after --:_default' \
    && ret=0
    ;;
    (remove)
    _arguments "${_arguments_options[@]}" : \
    '--timeout=[The time limit in whole seconds for one request, a login or an input. After a timeout, ncp cancels the request or stops the connection]:SECONDS:_default' \
    '--output-format=[The output format. Default\: text]:FORMAT:((text\:"Text for people"
    json\:"One JSON value on stdout, and JSON Lines on stderr"))' \
    '--config-file=[Use this configuration file in place of the global file\: ncp/mcp.json in the XDG configuration directory, by default ~/.config/ncp/mcp.json. The file must exist. ncp then does not search for a project .mcp.json, and runs the servers of this file with no trust question]:PATH:_files' \
    '--global[Remove the entry from the global file]' \
    '*-v[Show log lines on stderr\: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change]' \
    '*--verbose[Show log lines on stderr\: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change]' \
    '--timings[Show the time of each step on stderr\: CPU, configuration, credentials, server start, DNS, connect and each response]' \
    '(-v --verbose)*-q[Show no hints, no warnings and no progress on stderr. ncp still shows errors]' \
    '(-v --verbose)*--quiet[Show no hints, no warnings and no progress on stderr. ncp still shows errors]' \
    '--no-cache[Do not read or write the cache\: the protocol version and the tool list of each server]' \
    '--refresh[Do not read the cache, but write it\: ncp finds the protocol version and gets the tool list again. --no-cache has priority over --refresh]' \
    '-h[Print help (see more with '\''--help'\'')]' \
    '--help[Print help (see more with '\''--help'\'')]' \
    '-V[Print version]' \
    '--version[Print version]' \
    ':server -- The server name from the configuration:_ncp_servers' \
    && ret=0
    ;;
    (list)
    _arguments "${_arguments_options[@]}" : \
    '--timeout=[The time limit in whole seconds for one request, a login or an input. After a timeout, ncp cancels the request or stops the connection]:SECONDS:_default' \
    '--output-format=[The output format. Default\: text]:FORMAT:((text\:"Text for people"
    json\:"One JSON value on stdout, and JSON Lines on stderr"))' \
    '--config-file=[Use this configuration file in place of the global file\: ncp/mcp.json in the XDG configuration directory, by default ~/.config/ncp/mcp.json. The file must exist. ncp then does not search for a project .mcp.json, and runs the servers of this file with no trust question]:PATH:_files' \
    '*-v[Show log lines on stderr\: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change]' \
    '*--verbose[Show log lines on stderr\: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change]' \
    '--timings[Show the time of each step on stderr\: CPU, configuration, credentials, server start, DNS, connect and each response]' \
    '(-v --verbose)*-q[Show no hints, no warnings and no progress on stderr. ncp still shows errors]' \
    '(-v --verbose)*--quiet[Show no hints, no warnings and no progress on stderr. ncp still shows errors]' \
    '--no-cache[Do not read or write the cache\: the protocol version and the tool list of each server]' \
    '--refresh[Do not read the cache, but write it\: ncp finds the protocol version and gets the tool list again. --no-cache has priority over --refresh]' \
    '-h[Print help (see more with '\''--help'\'')]' \
    '--help[Print help (see more with '\''--help'\'')]' \
    '-V[Print version]' \
    '--version[Print version]' \
    && ret=0
    ;;
    (show)
    _arguments "${_arguments_options[@]}" : \
    '--timeout=[The time limit in whole seconds for one request, a login or an input. After a timeout, ncp cancels the request or stops the connection]:SECONDS:_default' \
    '--output-format=[The output format. Default\: text]:FORMAT:((text\:"Text for people"
    json\:"One JSON value on stdout, and JSON Lines on stderr"))' \
    '--config-file=[Use this configuration file in place of the global file\: ncp/mcp.json in the XDG configuration directory, by default ~/.config/ncp/mcp.json. The file must exist. ncp then does not search for a project .mcp.json, and runs the servers of this file with no trust question]:PATH:_files' \
    '*-v[Show log lines on stderr\: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change]' \
    '*--verbose[Show log lines on stderr\: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change]' \
    '--timings[Show the time of each step on stderr\: CPU, configuration, credentials, server start, DNS, connect and each response]' \
    '(-v --verbose)*-q[Show no hints, no warnings and no progress on stderr. ncp still shows errors]' \
    '(-v --verbose)*--quiet[Show no hints, no warnings and no progress on stderr. ncp still shows errors]' \
    '--no-cache[Do not read or write the cache\: the protocol version and the tool list of each server]' \
    '--refresh[Do not read the cache, but write it\: ncp finds the protocol version and gets the tool list again. --no-cache has priority over --refresh]' \
    '-h[Print help (see more with '\''--help'\'')]' \
    '--help[Print help (see more with '\''--help'\'')]' \
    '-V[Print version]' \
    '--version[Print version]' \
    ':server -- The server name from the configuration:_ncp_servers' \
    && ret=0
    ;;
            esac
        ;;
    esac
    ;;
    (tool)
    _arguments "${_arguments_options[@]}" : \
    '--timeout=[The time limit in whole seconds for one request, a login or an input. After a timeout, ncp cancels the request or stops the connection]:SECONDS:_default' \
    '--output-format=[The output format. Default\: text]:FORMAT:((text\:"Text for people"
    json\:"One JSON value on stdout, and JSON Lines on stderr"))' \
    '--config-file=[Use this configuration file in place of the global file\: ncp/mcp.json in the XDG configuration directory, by default ~/.config/ncp/mcp.json. The file must exist. ncp then does not search for a project .mcp.json, and runs the servers of this file with no trust question]:PATH:_files' \
    '*-v[Show log lines on stderr\: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change]' \
    '*--verbose[Show log lines on stderr\: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change]' \
    '--timings[Show the time of each step on stderr\: CPU, configuration, credentials, server start, DNS, connect and each response]' \
    '(-v --verbose)*-q[Show no hints, no warnings and no progress on stderr. ncp still shows errors]' \
    '(-v --verbose)*--quiet[Show no hints, no warnings and no progress on stderr. ncp still shows errors]' \
    '--no-cache[Do not read or write the cache\: the protocol version and the tool list of each server]' \
    '--refresh[Do not read the cache, but write it\: ncp finds the protocol version and gets the tool list again. --no-cache has priority over --refresh]' \
    '-h[Print help (see more with '\''--help'\'')]' \
    '--help[Print help (see more with '\''--help'\'')]' \
    '-V[Print version]' \
    '--version[Print version]' \
    ":: :_ncp__subcmd__tool_commands" \
    "*::: :->tool" \
    && ret=0

        case $state in
        (tool)
            words=($line[1] "${words[@]}")
            (( CURRENT += 1 ))
            curcontext="${curcontext%:*:*}:ncp-tool-command-$line[1]:"
            case $line[1] in
                (list)
    _arguments "${_arguments_options[@]}" : \
    '--timeout=[The time limit in whole seconds for one request, a login or an input. After a timeout, ncp cancels the request or stops the connection]:SECONDS:_default' \
    '--output-format=[The output format. Default\: text]:FORMAT:((text\:"Text for people"
    json\:"One JSON value on stdout, and JSON Lines on stderr"))' \
    '--config-file=[Use this configuration file in place of the global file\: ncp/mcp.json in the XDG configuration directory, by default ~/.config/ncp/mcp.json. The file must exist. ncp then does not search for a project .mcp.json, and runs the servers of this file with no trust question]:PATH:_files' \
    '*-v[Show log lines on stderr\: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change]' \
    '*--verbose[Show log lines on stderr\: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change]' \
    '--timings[Show the time of each step on stderr\: CPU, configuration, credentials, server start, DNS, connect and each response]' \
    '(-v --verbose)*-q[Show no hints, no warnings and no progress on stderr. ncp still shows errors]' \
    '(-v --verbose)*--quiet[Show no hints, no warnings and no progress on stderr. ncp still shows errors]' \
    '--no-cache[Do not read or write the cache\: the protocol version and the tool list of each server]' \
    '--refresh[Do not read the cache, but write it\: ncp finds the protocol version and gets the tool list again. --no-cache has priority over --refresh]' \
    '-h[Print help (see more with '\''--help'\'')]' \
    '--help[Print help (see more with '\''--help'\'')]' \
    '-V[Print version]' \
    '--version[Print version]' \
    ':server -- The server name from the configuration:_ncp_servers' \
    && ret=0
    ;;
    (show)
    _arguments "${_arguments_options[@]}" : \
    '--timeout=[The time limit in whole seconds for one request, a login or an input. After a timeout, ncp cancels the request or stops the connection]:SECONDS:_default' \
    '--output-format=[The output format. Default\: text]:FORMAT:((text\:"Text for people"
    json\:"One JSON value on stdout, and JSON Lines on stderr"))' \
    '--config-file=[Use this configuration file in place of the global file\: ncp/mcp.json in the XDG configuration directory, by default ~/.config/ncp/mcp.json. The file must exist. ncp then does not search for a project .mcp.json, and runs the servers of this file with no trust question]:PATH:_files' \
    '*-v[Show log lines on stderr\: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change]' \
    '*--verbose[Show log lines on stderr\: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change]' \
    '--timings[Show the time of each step on stderr\: CPU, configuration, credentials, server start, DNS, connect and each response]' \
    '(-v --verbose)*-q[Show no hints, no warnings and no progress on stderr. ncp still shows errors]' \
    '(-v --verbose)*--quiet[Show no hints, no warnings and no progress on stderr. ncp still shows errors]' \
    '--no-cache[Do not read or write the cache\: the protocol version and the tool list of each server]' \
    '--refresh[Do not read the cache, but write it\: ncp finds the protocol version and gets the tool list again. --no-cache has priority over --refresh]' \
    '-h[Print help (see more with '\''--help'\'')]' \
    '--help[Print help (see more with '\''--help'\'')]' \
    '-V[Print version]' \
    '--version[Print version]' \
    ':server -- The server name from the configuration. In front of a tool name, join them with ., \: or /:_ncp_servers' \
    '::tool -- The tool name:_ncp_tools' \
    && ret=0
    ;;
    (call)
    _arguments "${_arguments_options[@]}" : \
    '--timeout=[The time limit in whole seconds for one request, a login or an input. After a timeout, ncp cancels the request or stops the connection]:SECONDS:_default' \
    '--output-format=[The output format. Default\: text]:FORMAT:((text\:"Text for people"
    json\:"One JSON value on stdout, and JSON Lines on stderr"))' \
    '--config-file=[Use this configuration file in place of the global file\: ncp/mcp.json in the XDG configuration directory, by default ~/.config/ncp/mcp.json. The file must exist. ncp then does not search for a project .mcp.json, and runs the servers of this file with no trust question]:PATH:_files' \
    '--no-validate[Do not check the arguments against the input schema of the tool]' \
    '-h[Show the tool, as ncp tool show does. With no tool, show this help]' \
    '--help[Show the tool, as ncp tool show does. With no tool, show this help]' \
    '*-v[Show log lines on stderr\: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change]' \
    '*--verbose[Show log lines on stderr\: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change]' \
    '--timings[Show the time of each step on stderr\: CPU, configuration, credentials, server start, DNS, connect and each response]' \
    '(-v --verbose)*-q[Show no hints, no warnings and no progress on stderr. ncp still shows errors]' \
    '(-v --verbose)*--quiet[Show no hints, no warnings and no progress on stderr. ncp still shows errors]' \
    '--no-cache[Do not read or write the cache\: the protocol version and the tool list of each server]' \
    '--refresh[Do not read the cache, but write it\: ncp finds the protocol version and gets the tool list again. --no-cache has priority over --refresh]' \
    '-V[Print version]' \
    '--version[Print version]' \
    '::server -- The server name from the configuration. In front of a tool name, join them with ., \: or /:_ncp_servers' \
    '::tool -- The tool name:_ncp_tools' \
    '*::arguments -- The tool arguments\: key=value pairs, one JSON object, or -:_default' \
    && ret=0
    ;;
            esac
        ;;
    esac
    ;;
    (resource)
    _arguments "${_arguments_options[@]}" : \
    '--timeout=[The time limit in whole seconds for one request, a login or an input. After a timeout, ncp cancels the request or stops the connection]:SECONDS:_default' \
    '--output-format=[The output format. Default\: text]:FORMAT:((text\:"Text for people"
    json\:"One JSON value on stdout, and JSON Lines on stderr"))' \
    '--config-file=[Use this configuration file in place of the global file\: ncp/mcp.json in the XDG configuration directory, by default ~/.config/ncp/mcp.json. The file must exist. ncp then does not search for a project .mcp.json, and runs the servers of this file with no trust question]:PATH:_files' \
    '*-v[Show log lines on stderr\: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change]' \
    '*--verbose[Show log lines on stderr\: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change]' \
    '--timings[Show the time of each step on stderr\: CPU, configuration, credentials, server start, DNS, connect and each response]' \
    '(-v --verbose)*-q[Show no hints, no warnings and no progress on stderr. ncp still shows errors]' \
    '(-v --verbose)*--quiet[Show no hints, no warnings and no progress on stderr. ncp still shows errors]' \
    '--no-cache[Do not read or write the cache\: the protocol version and the tool list of each server]' \
    '--refresh[Do not read the cache, but write it\: ncp finds the protocol version and gets the tool list again. --no-cache has priority over --refresh]' \
    '-h[Print help (see more with '\''--help'\'')]' \
    '--help[Print help (see more with '\''--help'\'')]' \
    '-V[Print version]' \
    '--version[Print version]' \
    ":: :_ncp__subcmd__resource_commands" \
    "*::: :->resource" \
    && ret=0

        case $state in
        (resource)
            words=($line[1] "${words[@]}")
            (( CURRENT += 1 ))
            curcontext="${curcontext%:*:*}:ncp-resource-command-$line[1]:"
            case $line[1] in
                (list)
    _arguments "${_arguments_options[@]}" : \
    '--timeout=[The time limit in whole seconds for one request, a login or an input. After a timeout, ncp cancels the request or stops the connection]:SECONDS:_default' \
    '--output-format=[The output format. Default\: text]:FORMAT:((text\:"Text for people"
    json\:"One JSON value on stdout, and JSON Lines on stderr"))' \
    '--config-file=[Use this configuration file in place of the global file\: ncp/mcp.json in the XDG configuration directory, by default ~/.config/ncp/mcp.json. The file must exist. ncp then does not search for a project .mcp.json, and runs the servers of this file with no trust question]:PATH:_files' \
    '*-v[Show log lines on stderr\: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change]' \
    '*--verbose[Show log lines on stderr\: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change]' \
    '--timings[Show the time of each step on stderr\: CPU, configuration, credentials, server start, DNS, connect and each response]' \
    '(-v --verbose)*-q[Show no hints, no warnings and no progress on stderr. ncp still shows errors]' \
    '(-v --verbose)*--quiet[Show no hints, no warnings and no progress on stderr. ncp still shows errors]' \
    '--no-cache[Do not read or write the cache\: the protocol version and the tool list of each server]' \
    '--refresh[Do not read the cache, but write it\: ncp finds the protocol version and gets the tool list again. --no-cache has priority over --refresh]' \
    '-h[Print help (see more with '\''--help'\'')]' \
    '--help[Print help (see more with '\''--help'\'')]' \
    '-V[Print version]' \
    '--version[Print version]' \
    ':server -- The server name from the configuration:_ncp_servers' \
    && ret=0
    ;;
    (read)
    _arguments "${_arguments_options[@]}" : \
    '--timeout=[The time limit in whole seconds for one request, a login or an input. After a timeout, ncp cancels the request or stops the connection]:SECONDS:_default' \
    '--output-format=[The output format. Default\: text]:FORMAT:((text\:"Text for people"
    json\:"One JSON value on stdout, and JSON Lines on stderr"))' \
    '--config-file=[Use this configuration file in place of the global file\: ncp/mcp.json in the XDG configuration directory, by default ~/.config/ncp/mcp.json. The file must exist. ncp then does not search for a project .mcp.json, and runs the servers of this file with no trust question]:PATH:_files' \
    '*-v[Show log lines on stderr\: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change]' \
    '*--verbose[Show log lines on stderr\: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change]' \
    '--timings[Show the time of each step on stderr\: CPU, configuration, credentials, server start, DNS, connect and each response]' \
    '(-v --verbose)*-q[Show no hints, no warnings and no progress on stderr. ncp still shows errors]' \
    '(-v --verbose)*--quiet[Show no hints, no warnings and no progress on stderr. ncp still shows errors]' \
    '--no-cache[Do not read or write the cache\: the protocol version and the tool list of each server]' \
    '--refresh[Do not read the cache, but write it\: ncp finds the protocol version and gets the tool list again. --no-cache has priority over --refresh]' \
    '-h[Print help (see more with '\''--help'\'')]' \
    '--help[Print help (see more with '\''--help'\'')]' \
    '-V[Print version]' \
    '--version[Print version]' \
    ':server -- The server name from the configuration:_ncp_servers' \
    ':uri -- The resource URI:_default' \
    && ret=0
    ;;
            esac
        ;;
    esac
    ;;
    (prompt)
    _arguments "${_arguments_options[@]}" : \
    '--timeout=[The time limit in whole seconds for one request, a login or an input. After a timeout, ncp cancels the request or stops the connection]:SECONDS:_default' \
    '--output-format=[The output format. Default\: text]:FORMAT:((text\:"Text for people"
    json\:"One JSON value on stdout, and JSON Lines on stderr"))' \
    '--config-file=[Use this configuration file in place of the global file\: ncp/mcp.json in the XDG configuration directory, by default ~/.config/ncp/mcp.json. The file must exist. ncp then does not search for a project .mcp.json, and runs the servers of this file with no trust question]:PATH:_files' \
    '*-v[Show log lines on stderr\: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change]' \
    '*--verbose[Show log lines on stderr\: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change]' \
    '--timings[Show the time of each step on stderr\: CPU, configuration, credentials, server start, DNS, connect and each response]' \
    '(-v --verbose)*-q[Show no hints, no warnings and no progress on stderr. ncp still shows errors]' \
    '(-v --verbose)*--quiet[Show no hints, no warnings and no progress on stderr. ncp still shows errors]' \
    '--no-cache[Do not read or write the cache\: the protocol version and the tool list of each server]' \
    '--refresh[Do not read the cache, but write it\: ncp finds the protocol version and gets the tool list again. --no-cache has priority over --refresh]' \
    '-h[Print help (see more with '\''--help'\'')]' \
    '--help[Print help (see more with '\''--help'\'')]' \
    '-V[Print version]' \
    '--version[Print version]' \
    ":: :_ncp__subcmd__prompt_commands" \
    "*::: :->prompt" \
    && ret=0

        case $state in
        (prompt)
            words=($line[1] "${words[@]}")
            (( CURRENT += 1 ))
            curcontext="${curcontext%:*:*}:ncp-prompt-command-$line[1]:"
            case $line[1] in
                (list)
    _arguments "${_arguments_options[@]}" : \
    '--timeout=[The time limit in whole seconds for one request, a login or an input. After a timeout, ncp cancels the request or stops the connection]:SECONDS:_default' \
    '--output-format=[The output format. Default\: text]:FORMAT:((text\:"Text for people"
    json\:"One JSON value on stdout, and JSON Lines on stderr"))' \
    '--config-file=[Use this configuration file in place of the global file\: ncp/mcp.json in the XDG configuration directory, by default ~/.config/ncp/mcp.json. The file must exist. ncp then does not search for a project .mcp.json, and runs the servers of this file with no trust question]:PATH:_files' \
    '*-v[Show log lines on stderr\: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change]' \
    '*--verbose[Show log lines on stderr\: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change]' \
    '--timings[Show the time of each step on stderr\: CPU, configuration, credentials, server start, DNS, connect and each response]' \
    '(-v --verbose)*-q[Show no hints, no warnings and no progress on stderr. ncp still shows errors]' \
    '(-v --verbose)*--quiet[Show no hints, no warnings and no progress on stderr. ncp still shows errors]' \
    '--no-cache[Do not read or write the cache\: the protocol version and the tool list of each server]' \
    '--refresh[Do not read the cache, but write it\: ncp finds the protocol version and gets the tool list again. --no-cache has priority over --refresh]' \
    '-h[Print help (see more with '\''--help'\'')]' \
    '--help[Print help (see more with '\''--help'\'')]' \
    '-V[Print version]' \
    '--version[Print version]' \
    ':server -- The server name from the configuration:_ncp_servers' \
    && ret=0
    ;;
    (get)
    _arguments "${_arguments_options[@]}" : \
    '--timeout=[The time limit in whole seconds for one request, a login or an input. After a timeout, ncp cancels the request or stops the connection]:SECONDS:_default' \
    '--output-format=[The output format. Default\: text]:FORMAT:((text\:"Text for people"
    json\:"One JSON value on stdout, and JSON Lines on stderr"))' \
    '--config-file=[Use this configuration file in place of the global file\: ncp/mcp.json in the XDG configuration directory, by default ~/.config/ncp/mcp.json. The file must exist. ncp then does not search for a project .mcp.json, and runs the servers of this file with no trust question]:PATH:_files' \
    '*-v[Show log lines on stderr\: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change]' \
    '*--verbose[Show log lines on stderr\: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change]' \
    '--timings[Show the time of each step on stderr\: CPU, configuration, credentials, server start, DNS, connect and each response]' \
    '(-v --verbose)*-q[Show no hints, no warnings and no progress on stderr. ncp still shows errors]' \
    '(-v --verbose)*--quiet[Show no hints, no warnings and no progress on stderr. ncp still shows errors]' \
    '--no-cache[Do not read or write the cache\: the protocol version and the tool list of each server]' \
    '--refresh[Do not read the cache, but write it\: ncp finds the protocol version and gets the tool list again. --no-cache has priority over --refresh]' \
    '-h[Print help (see more with '\''--help'\'')]' \
    '--help[Print help (see more with '\''--help'\'')]' \
    '-V[Print version]' \
    '--version[Print version]' \
    ':server -- The server name from the configuration. In front of a prompt name, join them with ., \: or /:_ncp_servers' \
    '::prompt -- The prompt name:_default' \
    '*::arguments -- The prompt arguments\: key=value pairs:_default' \
    && ret=0
    ;;
            esac
        ;;
    esac
    ;;
    (auth)
    _arguments "${_arguments_options[@]}" : \
    '--timeout=[The time limit in whole seconds for one request, a login or an input. After a timeout, ncp cancels the request or stops the connection]:SECONDS:_default' \
    '--output-format=[The output format. Default\: text]:FORMAT:((text\:"Text for people"
    json\:"One JSON value on stdout, and JSON Lines on stderr"))' \
    '--config-file=[Use this configuration file in place of the global file\: ncp/mcp.json in the XDG configuration directory, by default ~/.config/ncp/mcp.json. The file must exist. ncp then does not search for a project .mcp.json, and runs the servers of this file with no trust question]:PATH:_files' \
    '*-v[Show log lines on stderr\: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change]' \
    '*--verbose[Show log lines on stderr\: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change]' \
    '--timings[Show the time of each step on stderr\: CPU, configuration, credentials, server start, DNS, connect and each response]' \
    '(-v --verbose)*-q[Show no hints, no warnings and no progress on stderr. ncp still shows errors]' \
    '(-v --verbose)*--quiet[Show no hints, no warnings and no progress on stderr. ncp still shows errors]' \
    '--no-cache[Do not read or write the cache\: the protocol version and the tool list of each server]' \
    '--refresh[Do not read the cache, but write it\: ncp finds the protocol version and gets the tool list again. --no-cache has priority over --refresh]' \
    '-h[Print help (see more with '\''--help'\'')]' \
    '--help[Print help (see more with '\''--help'\'')]' \
    '-V[Print version]' \
    '--version[Print version]' \
    ":: :_ncp__subcmd__auth_commands" \
    "*::: :->auth" \
    && ret=0

        case $state in
        (auth)
            words=($line[1] "${words[@]}")
            (( CURRENT += 1 ))
            curcontext="${curcontext%:*:*}:ncp-auth-command-$line[1]:"
            case $line[1] in
                (login)
    _arguments "${_arguments_options[@]}" : \
    '--credential-store=[The credential store for the tokens]:CREDENTIAL_STORE:((native\:"The native store\: the macOS Keychain or the Linux Secret Service"
    file\:"The credential file, with mode 0600"))' \
    '--timeout=[The time limit in whole seconds for one request, a login or an input. After a timeout, ncp cancels the request or stops the connection]:SECONDS:_default' \
    '--output-format=[The output format. Default\: text]:FORMAT:((text\:"Text for people"
    json\:"One JSON value on stdout, and JSON Lines on stderr"))' \
    '--config-file=[Use this configuration file in place of the global file\: ncp/mcp.json in the XDG configuration directory, by default ~/.config/ncp/mcp.json. The file must exist. ncp then does not search for a project .mcp.json, and runs the servers of this file with no trust question]:PATH:_files' \
    '--no-browser[Do not open a browser\: show the URL, and read the redirect URL from stdin]' \
    '*-v[Show log lines on stderr\: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change]' \
    '*--verbose[Show log lines on stderr\: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change]' \
    '--timings[Show the time of each step on stderr\: CPU, configuration, credentials, server start, DNS, connect and each response]' \
    '(-v --verbose)*-q[Show no hints, no warnings and no progress on stderr. ncp still shows errors]' \
    '(-v --verbose)*--quiet[Show no hints, no warnings and no progress on stderr. ncp still shows errors]' \
    '--no-cache[Do not read or write the cache\: the protocol version and the tool list of each server]' \
    '--refresh[Do not read the cache, but write it\: ncp finds the protocol version and gets the tool list again. --no-cache has priority over --refresh]' \
    '-h[Print help (see more with '\''--help'\'')]' \
    '--help[Print help (see more with '\''--help'\'')]' \
    '-V[Print version]' \
    '--version[Print version]' \
    ':server -- The server name from the configuration:_ncp_servers' \
    && ret=0
    ;;
    (logout)
    _arguments "${_arguments_options[@]}" : \
    '--timeout=[The time limit in whole seconds for one request, a login or an input. After a timeout, ncp cancels the request or stops the connection]:SECONDS:_default' \
    '--output-format=[The output format. Default\: text]:FORMAT:((text\:"Text for people"
    json\:"One JSON value on stdout, and JSON Lines on stderr"))' \
    '--config-file=[Use this configuration file in place of the global file\: ncp/mcp.json in the XDG configuration directory, by default ~/.config/ncp/mcp.json. The file must exist. ncp then does not search for a project .mcp.json, and runs the servers of this file with no trust question]:PATH:_files' \
    '*-v[Show log lines on stderr\: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change]' \
    '*--verbose[Show log lines on stderr\: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change]' \
    '--timings[Show the time of each step on stderr\: CPU, configuration, credentials, server start, DNS, connect and each response]' \
    '(-v --verbose)*-q[Show no hints, no warnings and no progress on stderr. ncp still shows errors]' \
    '(-v --verbose)*--quiet[Show no hints, no warnings and no progress on stderr. ncp still shows errors]' \
    '--no-cache[Do not read or write the cache\: the protocol version and the tool list of each server]' \
    '--refresh[Do not read the cache, but write it\: ncp finds the protocol version and gets the tool list again. --no-cache has priority over --refresh]' \
    '-h[Print help (see more with '\''--help'\'')]' \
    '--help[Print help (see more with '\''--help'\'')]' \
    '-V[Print version]' \
    '--version[Print version]' \
    ':server -- The server name from the configuration:_ncp_servers' \
    && ret=0
    ;;
    (status)
    _arguments "${_arguments_options[@]}" : \
    '--timeout=[The time limit in whole seconds for one request, a login or an input. After a timeout, ncp cancels the request or stops the connection]:SECONDS:_default' \
    '--output-format=[The output format. Default\: text]:FORMAT:((text\:"Text for people"
    json\:"One JSON value on stdout, and JSON Lines on stderr"))' \
    '--config-file=[Use this configuration file in place of the global file\: ncp/mcp.json in the XDG configuration directory, by default ~/.config/ncp/mcp.json. The file must exist. ncp then does not search for a project .mcp.json, and runs the servers of this file with no trust question]:PATH:_files' \
    '*-v[Show log lines on stderr\: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change]' \
    '*--verbose[Show log lines on stderr\: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change]' \
    '--timings[Show the time of each step on stderr\: CPU, configuration, credentials, server start, DNS, connect and each response]' \
    '(-v --verbose)*-q[Show no hints, no warnings and no progress on stderr. ncp still shows errors]' \
    '(-v --verbose)*--quiet[Show no hints, no warnings and no progress on stderr. ncp still shows errors]' \
    '--no-cache[Do not read or write the cache\: the protocol version and the tool list of each server]' \
    '--refresh[Do not read the cache, but write it\: ncp finds the protocol version and gets the tool list again. --no-cache has priority over --refresh]' \
    '-h[Print help (see more with '\''--help'\'')]' \
    '--help[Print help (see more with '\''--help'\'')]' \
    '-V[Print version]' \
    '--version[Print version]' \
    '::server -- The server name from the configuration:_ncp_servers' \
    && ret=0
    ;;
            esac
        ;;
    esac
    ;;
    (secret)
    _arguments "${_arguments_options[@]}" : \
    '--timeout=[The time limit in whole seconds for one request, a login or an input. After a timeout, ncp cancels the request or stops the connection]:SECONDS:_default' \
    '--output-format=[The output format. Default\: text]:FORMAT:((text\:"Text for people"
    json\:"One JSON value on stdout, and JSON Lines on stderr"))' \
    '--config-file=[Use this configuration file in place of the global file\: ncp/mcp.json in the XDG configuration directory, by default ~/.config/ncp/mcp.json. The file must exist. ncp then does not search for a project .mcp.json, and runs the servers of this file with no trust question]:PATH:_files' \
    '*-v[Show log lines on stderr\: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change]' \
    '*--verbose[Show log lines on stderr\: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change]' \
    '--timings[Show the time of each step on stderr\: CPU, configuration, credentials, server start, DNS, connect and each response]' \
    '(-v --verbose)*-q[Show no hints, no warnings and no progress on stderr. ncp still shows errors]' \
    '(-v --verbose)*--quiet[Show no hints, no warnings and no progress on stderr. ncp still shows errors]' \
    '--no-cache[Do not read or write the cache\: the protocol version and the tool list of each server]' \
    '--refresh[Do not read the cache, but write it\: ncp finds the protocol version and gets the tool list again. --no-cache has priority over --refresh]' \
    '-h[Print help (see more with '\''--help'\'')]' \
    '--help[Print help (see more with '\''--help'\'')]' \
    '-V[Print version]' \
    '--version[Print version]' \
    ":: :_ncp__subcmd__secret_commands" \
    "*::: :->secret" \
    && ret=0

        case $state in
        (secret)
            words=($line[1] "${words[@]}")
            (( CURRENT += 1 ))
            curcontext="${curcontext%:*:*}:ncp-secret-command-$line[1]:"
            case $line[1] in
                (set)
    _arguments "${_arguments_options[@]}" : \
    '--server=[Use only the destination of this server]:SERVER:_default' \
    '--credential-store=[The credential store for the value]:CREDENTIAL_STORE:((native\:"The native store\: the macOS Keychain or the Linux Secret Service"
    file\:"The credential file, with mode 0600"))' \
    '--timeout=[The time limit in whole seconds for one request, a login or an input. After a timeout, ncp cancels the request or stops the connection]:SECONDS:_default' \
    '--output-format=[The output format. Default\: text]:FORMAT:((text\:"Text for people"
    json\:"One JSON value on stdout, and JSON Lines on stderr"))' \
    '--config-file=[Use this configuration file in place of the global file\: ncp/mcp.json in the XDG configuration directory, by default ~/.config/ncp/mcp.json. The file must exist. ncp then does not search for a project .mcp.json, and runs the servers of this file with no trust question]:PATH:_files' \
    '*-v[Show log lines on stderr\: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change]' \
    '*--verbose[Show log lines on stderr\: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change]' \
    '--timings[Show the time of each step on stderr\: CPU, configuration, credentials, server start, DNS, connect and each response]' \
    '(-v --verbose)*-q[Show no hints, no warnings and no progress on stderr. ncp still shows errors]' \
    '(-v --verbose)*--quiet[Show no hints, no warnings and no progress on stderr. ncp still shows errors]' \
    '--no-cache[Do not read or write the cache\: the protocol version and the tool list of each server]' \
    '--refresh[Do not read the cache, but write it\: ncp finds the protocol version and gets the tool list again. --no-cache has priority over --refresh]' \
    '-h[Print help (see more with '\''--help'\'')]' \
    '--help[Print help (see more with '\''--help'\'')]' \
    '-V[Print version]' \
    '--version[Print version]' \
    ':name -- The name, as in ${NAME} in the configuration:_default' \
    && ret=0
    ;;
    (list)
    _arguments "${_arguments_options[@]}" : \
    '--timeout=[The time limit in whole seconds for one request, a login or an input. After a timeout, ncp cancels the request or stops the connection]:SECONDS:_default' \
    '--output-format=[The output format. Default\: text]:FORMAT:((text\:"Text for people"
    json\:"One JSON value on stdout, and JSON Lines on stderr"))' \
    '--config-file=[Use this configuration file in place of the global file\: ncp/mcp.json in the XDG configuration directory, by default ~/.config/ncp/mcp.json. The file must exist. ncp then does not search for a project .mcp.json, and runs the servers of this file with no trust question]:PATH:_files' \
    '*-v[Show log lines on stderr\: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change]' \
    '*--verbose[Show log lines on stderr\: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change]' \
    '--timings[Show the time of each step on stderr\: CPU, configuration, credentials, server start, DNS, connect and each response]' \
    '(-v --verbose)*-q[Show no hints, no warnings and no progress on stderr. ncp still shows errors]' \
    '(-v --verbose)*--quiet[Show no hints, no warnings and no progress on stderr. ncp still shows errors]' \
    '--no-cache[Do not read or write the cache\: the protocol version and the tool list of each server]' \
    '--refresh[Do not read the cache, but write it\: ncp finds the protocol version and gets the tool list again. --no-cache has priority over --refresh]' \
    '-h[Print help (see more with '\''--help'\'')]' \
    '--help[Print help (see more with '\''--help'\'')]' \
    '-V[Print version]' \
    '--version[Print version]' \
    && ret=0
    ;;
    (delete)
    _arguments "${_arguments_options[@]}" : \
    '--server=[Delete only the item of the destination of this server]:SERVER:_default' \
    '--timeout=[The time limit in whole seconds for one request, a login or an input. After a timeout, ncp cancels the request or stops the connection]:SECONDS:_default' \
    '--output-format=[The output format. Default\: text]:FORMAT:((text\:"Text for people"
    json\:"One JSON value on stdout, and JSON Lines on stderr"))' \
    '--config-file=[Use this configuration file in place of the global file\: ncp/mcp.json in the XDG configuration directory, by default ~/.config/ncp/mcp.json. The file must exist. ncp then does not search for a project .mcp.json, and runs the servers of this file with no trust question]:PATH:_files' \
    '--unused[Delete the items that no server of the global file or of the project file of the current directory uses. A project in another directory can still use them]' \
    '--all[Delete the items of all destinations]' \
    '*-v[Show log lines on stderr\: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change]' \
    '*--verbose[Show log lines on stderr\: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change]' \
    '--timings[Show the time of each step on stderr\: CPU, configuration, credentials, server start, DNS, connect and each response]' \
    '(-v --verbose)*-q[Show no hints, no warnings and no progress on stderr. ncp still shows errors]' \
    '(-v --verbose)*--quiet[Show no hints, no warnings and no progress on stderr. ncp still shows errors]' \
    '--no-cache[Do not read or write the cache\: the protocol version and the tool list of each server]' \
    '--refresh[Do not read the cache, but write it\: ncp finds the protocol version and gets the tool list again. --no-cache has priority over --refresh]' \
    '-h[Print help (see more with '\''--help'\'')]' \
    '--help[Print help (see more with '\''--help'\'')]' \
    '-V[Print version]' \
    '--version[Print version]' \
    ':name -- The name, as in ${NAME} in the configuration:_default' \
    && ret=0
    ;;
            esac
        ;;
    esac
    ;;
    (trust)
    _arguments "${_arguments_options[@]}" : \
    '--timeout=[The time limit in whole seconds for one request, a login or an input. After a timeout, ncp cancels the request or stops the connection]:SECONDS:_default' \
    '--output-format=[The output format. Default\: text]:FORMAT:((text\:"Text for people"
    json\:"One JSON value on stdout, and JSON Lines on stderr"))' \
    '--config-file=[Use this configuration file in place of the global file\: ncp/mcp.json in the XDG configuration directory, by default ~/.config/ncp/mcp.json. The file must exist. ncp then does not search for a project .mcp.json, and runs the servers of this file with no trust question]:PATH:_files' \
    '--show[Show the entries and the trust state, and change nothing]' \
    '--untrust[Remove the trust of the project file]' \
    '--ignore[Ignore the project file\: ncp does not ask again until you run ncp trust]' \
    '*-v[Show log lines on stderr\: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change]' \
    '*--verbose[Show log lines on stderr\: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change]' \
    '--timings[Show the time of each step on stderr\: CPU, configuration, credentials, server start, DNS, connect and each response]' \
    '(-v --verbose)*-q[Show no hints, no warnings and no progress on stderr. ncp still shows errors]' \
    '(-v --verbose)*--quiet[Show no hints, no warnings and no progress on stderr. ncp still shows errors]' \
    '--no-cache[Do not read or write the cache\: the protocol version and the tool list of each server]' \
    '--refresh[Do not read the cache, but write it\: ncp finds the protocol version and gets the tool list again. --no-cache has priority over --refresh]' \
    '-h[Print help (see more with '\''--help'\'')]' \
    '--help[Print help (see more with '\''--help'\'')]' \
    '-V[Print version]' \
    '--version[Print version]' \
    && ret=0
    ;;
    (generate-shell-completion)
    _arguments "${_arguments_options[@]}" : \
    '--timeout=[The time limit in whole seconds for one request, a login or an input. After a timeout, ncp cancels the request or stops the connection]:SECONDS:_default' \
    '--output-format=[The output format. Default\: text]:FORMAT:((text\:"Text for people"
    json\:"One JSON value on stdout, and JSON Lines on stderr"))' \
    '--config-file=[Use this configuration file in place of the global file\: ncp/mcp.json in the XDG configuration directory, by default ~/.config/ncp/mcp.json. The file must exist. ncp then does not search for a project .mcp.json, and runs the servers of this file with no trust question]:PATH:_files' \
    '*-v[Show log lines on stderr\: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change]' \
    '*--verbose[Show log lines on stderr\: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change]' \
    '--timings[Show the time of each step on stderr\: CPU, configuration, credentials, server start, DNS, connect and each response]' \
    '(-v --verbose)*-q[Show no hints, no warnings and no progress on stderr. ncp still shows errors]' \
    '(-v --verbose)*--quiet[Show no hints, no warnings and no progress on stderr. ncp still shows errors]' \
    '--no-cache[Do not read or write the cache\: the protocol version and the tool list of each server]' \
    '--refresh[Do not read the cache, but write it\: ncp finds the protocol version and gets the tool list again. --no-cache has priority over --refresh]' \
    '-h[Print help (see more with '\''--help'\'')]' \
    '--help[Print help (see more with '\''--help'\'')]' \
    '-V[Print version]' \
    '--version[Print version]' \
    ':shell -- The shell:(bash zsh fish)' \
    && ret=0
    ;;
            esac
        ;;
    esac
    }

    (( $+functions[_ncp_commands] )) ||
    _ncp_commands() {
        local commands; commands=(
    'server:Configured servers' \
    'tool:Tools of a server' \
    'resource:Resources of a server' \
    'prompt:Prompts of a server' \
    'auth:OAuth login of HTTP servers' \
    'secret:Secrets for \${NAME} in the configuration' \
    'trust:Trust the project .mcp.json of the current directory' \
    'generate-shell-completion:Show the completion script for a shell' \
        )
        _describe -t commands 'ncp commands' commands "$@"
    }
    (( $+functions[_ncp__subcmd__auth_commands] )) ||
    _ncp__subcmd__auth_commands() {
        local commands; commands=(
    'login:Log in to an HTTP server with OAuth and store the tokens' \
    'logout:Delete the OAuth tokens of a server from all credential stores' \
    'status:Show the OAuth login of each server, or of one server' \
        )
        _describe -t commands 'ncp auth commands' commands "$@"
    }
    (( $+functions[_ncp__subcmd__auth__subcmd__login_commands] )) ||
    _ncp__subcmd__auth__subcmd__login_commands() {
        local commands; commands=()
        _describe -t commands 'ncp auth login commands' commands "$@"
    }
    (( $+functions[_ncp__subcmd__auth__subcmd__logout_commands] )) ||
    _ncp__subcmd__auth__subcmd__logout_commands() {
        local commands; commands=()
        _describe -t commands 'ncp auth logout commands' commands "$@"
    }
    (( $+functions[_ncp__subcmd__auth__subcmd__status_commands] )) ||
    _ncp__subcmd__auth__subcmd__status_commands() {
        local commands; commands=()
        _describe -t commands 'ncp auth status commands' commands "$@"
    }
    (( $+functions[_ncp__subcmd__generate-shell-completion_commands] )) ||
    _ncp__subcmd__generate-shell-completion_commands() {
        local commands; commands=()
        _describe -t commands 'ncp generate-shell-completion commands' commands "$@"
    }
    (( $+functions[_ncp__subcmd__prompt_commands] )) ||
    _ncp__subcmd__prompt_commands() {
        local commands; commands=(
    'list:List the prompts of a server' \
    'get:Get one prompt' \
        )
        _describe -t commands 'ncp prompt commands' commands "$@"
    }
    (( $+functions[_ncp__subcmd__prompt__subcmd__get_commands] )) ||
    _ncp__subcmd__prompt__subcmd__get_commands() {
        local commands; commands=()
        _describe -t commands 'ncp prompt get commands' commands "$@"
    }
    (( $+functions[_ncp__subcmd__prompt__subcmd__list_commands] )) ||
    _ncp__subcmd__prompt__subcmd__list_commands() {
        local commands; commands=()
        _describe -t commands 'ncp prompt list commands' commands "$@"
    }
    (( $+functions[_ncp__subcmd__resource_commands] )) ||
    _ncp__subcmd__resource_commands() {
        local commands; commands=(
    'list:List the resources and the resource templates of a server' \
    'read:Read one resource' \
        )
        _describe -t commands 'ncp resource commands' commands "$@"
    }
    (( $+functions[_ncp__subcmd__resource__subcmd__list_commands] )) ||
    _ncp__subcmd__resource__subcmd__list_commands() {
        local commands; commands=()
        _describe -t commands 'ncp resource list commands' commands "$@"
    }
    (( $+functions[_ncp__subcmd__resource__subcmd__read_commands] )) ||
    _ncp__subcmd__resource__subcmd__read_commands() {
        local commands; commands=()
        _describe -t commands 'ncp resource read commands' commands "$@"
    }
    (( $+functions[_ncp__subcmd__secret_commands] )) ||
    _ncp__subcmd__secret_commands() {
        local commands; commands=(
    'set:Store a value for \${NAME}' \
    'list:List the stored secrets' \
    'delete:Delete a stored secret from all credential stores' \
        )
        _describe -t commands 'ncp secret commands' commands "$@"
    }
    (( $+functions[_ncp__subcmd__secret__subcmd__delete_commands] )) ||
    _ncp__subcmd__secret__subcmd__delete_commands() {
        local commands; commands=()
        _describe -t commands 'ncp secret delete commands' commands "$@"
    }
    (( $+functions[_ncp__subcmd__secret__subcmd__list_commands] )) ||
    _ncp__subcmd__secret__subcmd__list_commands() {
        local commands; commands=()
        _describe -t commands 'ncp secret list commands' commands "$@"
    }
    (( $+functions[_ncp__subcmd__secret__subcmd__set_commands] )) ||
    _ncp__subcmd__secret__subcmd__set_commands() {
        local commands; commands=()
        _describe -t commands 'ncp secret set commands' commands "$@"
    }
    (( $+functions[_ncp__subcmd__server_commands] )) ||
    _ncp__subcmd__server_commands() {
        local commands; commands=(
    'add:Add a server to the project .mcp.json or the global file' \
    'remove:Remove a server from the file that gives it' \
    'list:List the servers of the global file and of the project file' \
    'show:Show the information and the instructions that the server gives in its handshake' \
        )
        _describe -t commands 'ncp server commands' commands "$@"
    }
    (( $+functions[_ncp__subcmd__server__subcmd__add_commands] )) ||
    _ncp__subcmd__server__subcmd__add_commands() {
        local commands; commands=()
        _describe -t commands 'ncp server add commands' commands "$@"
    }
    (( $+functions[_ncp__subcmd__server__subcmd__list_commands] )) ||
    _ncp__subcmd__server__subcmd__list_commands() {
        local commands; commands=()
        _describe -t commands 'ncp server list commands' commands "$@"
    }
    (( $+functions[_ncp__subcmd__server__subcmd__remove_commands] )) ||
    _ncp__subcmd__server__subcmd__remove_commands() {
        local commands; commands=()
        _describe -t commands 'ncp server remove commands' commands "$@"
    }
    (( $+functions[_ncp__subcmd__server__subcmd__show_commands] )) ||
    _ncp__subcmd__server__subcmd__show_commands() {
        local commands; commands=()
        _describe -t commands 'ncp server show commands' commands "$@"
    }
    (( $+functions[_ncp__subcmd__tool_commands] )) ||
    _ncp__subcmd__tool_commands() {
        local commands; commands=(
    'list:List the tools of a server' \
    'show:Show one tool\: its arguments, output and an example call' \
    'call:Call a tool' \
        )
        _describe -t commands 'ncp tool commands' commands "$@"
    }
    (( $+functions[_ncp__subcmd__tool__subcmd__call_commands] )) ||
    _ncp__subcmd__tool__subcmd__call_commands() {
        local commands; commands=()
        _describe -t commands 'ncp tool call commands' commands "$@"
    }
    (( $+functions[_ncp__subcmd__tool__subcmd__list_commands] )) ||
    _ncp__subcmd__tool__subcmd__list_commands() {
        local commands; commands=()
        _describe -t commands 'ncp tool list commands' commands "$@"
    }
    (( $+functions[_ncp__subcmd__tool__subcmd__show_commands] )) ||
    _ncp__subcmd__tool__subcmd__show_commands() {
        local commands; commands=()
        _describe -t commands 'ncp tool show commands' commands "$@"
    }
    (( $+functions[_ncp__subcmd__trust_commands] )) ||
    _ncp__subcmd__trust_commands() {
        local commands; commands=()
        _describe -t commands 'ncp trust commands' commands "$@"
    }

    if [ "$funcstack[1]" = "_ncp" ]; then
        _ncp "$@"
    else
        compdef _ncp ncp
    fi
    "#);
}

#[test]
fn completion_script_for_fish() {
    let home = Home::new("completion-snapshot-fish");
    ncp_snapshot!(home.filters(), home.ncp(["generate-shell-completion", "fish"]), @r#"
    exit_code: 0 (success)
    ----- stdout -----
    # Print an optspec for argparse to handle cmd's options that are independent of any subcommand.
    function __fish_ncp_global_optspecs
        string join \n timeout= v/verbose timings q/quiet no-cache refresh output-format= config-file= h/help V/version
    end

    function __fish_ncp_needs_command
        # Figure out if the current invocation already has a command.
        set -l cmd (commandline -opc)
        set -e cmd[1]
        argparse -s (__fish_ncp_global_optspecs) -- $cmd 2>/dev/null
        or return
        if set -q argv[1]
            # Also print the command, so this can be used to figure out what it is.
            echo $argv[1]
            return 1
        end
        return 0
    end

    function __fish_ncp_using_subcommand
        set -l cmd (__fish_ncp_needs_command)
        test -z "$cmd"
        and return 1
        contains -- $cmd[1] $argv
    end

    complete -c ncp -n "__fish_ncp_needs_command" -l timeout -d 'The time limit in whole seconds for one request, a login or an input. After a timeout, ncp cancels the request or stops the connection' -r
    complete -c ncp -n "__fish_ncp_needs_command" -l output-format -d 'The output format. Default: text' -r -f -a "text\t'Text for people'
    json\t'One JSON value on stdout, and JSON Lines on stderr'"
    complete -c ncp -n "__fish_ncp_needs_command" -l config-file -d 'Use this configuration file in place of the global file: ncp/mcp.json in the XDG configuration directory, by default ~/.config/ncp/mcp.json. The file must exist. ncp then does not search for a project .mcp.json, and runs the servers of this file with no trust question' -r -F
    complete -c ncp -n "__fish_ncp_needs_command" -s v -l verbose -d 'Show log lines on stderr: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change'
    complete -c ncp -n "__fish_ncp_needs_command" -l timings -d 'Show the time of each step on stderr: CPU, configuration, credentials, server start, DNS, connect and each response'
    complete -c ncp -n "__fish_ncp_needs_command" -s q -l quiet -d 'Show no hints, no warnings and no progress on stderr. ncp still shows errors'
    complete -c ncp -n "__fish_ncp_needs_command" -l no-cache -d 'Do not read or write the cache: the protocol version and the tool list of each server'
    complete -c ncp -n "__fish_ncp_needs_command" -l refresh -d 'Do not read the cache, but write it: ncp finds the protocol version and gets the tool list again. --no-cache has priority over --refresh'
    complete -c ncp -n "__fish_ncp_needs_command" -s h -l help -d 'Print help (see more with \'--help\')'
    complete -c ncp -n "__fish_ncp_needs_command" -s V -l version -d 'Print version'
    complete -c ncp -n "__fish_ncp_needs_command" -f -a "server" -d 'Configured servers'
    complete -c ncp -n "__fish_ncp_needs_command" -f -a "tool" -d 'Tools of a server'
    complete -c ncp -n "__fish_ncp_needs_command" -f -a "resource" -d 'Resources of a server'
    complete -c ncp -n "__fish_ncp_needs_command" -f -a "prompt" -d 'Prompts of a server'
    complete -c ncp -n "__fish_ncp_needs_command" -f -a "auth" -d 'OAuth login of HTTP servers'
    complete -c ncp -n "__fish_ncp_needs_command" -f -a "secret" -d 'Secrets for ${NAME} in the configuration'
    complete -c ncp -n "__fish_ncp_needs_command" -f -a "trust" -d 'Trust the project .mcp.json of the current directory'
    complete -c ncp -n "__fish_ncp_needs_command" -f -a "generate-shell-completion" -d 'Show the completion script for a shell'
    complete -c ncp -n "__fish_ncp_using_subcommand server; and not __fish_seen_subcommand_from add remove list show" -l timeout -d 'The time limit in whole seconds for one request, a login or an input. After a timeout, ncp cancels the request or stops the connection' -r
    complete -c ncp -n "__fish_ncp_using_subcommand server; and not __fish_seen_subcommand_from add remove list show" -l output-format -d 'The output format. Default: text' -r -f -a "text\t'Text for people'
    json\t'One JSON value on stdout, and JSON Lines on stderr'"
    complete -c ncp -n "__fish_ncp_using_subcommand server; and not __fish_seen_subcommand_from add remove list show" -l config-file -d 'Use this configuration file in place of the global file: ncp/mcp.json in the XDG configuration directory, by default ~/.config/ncp/mcp.json. The file must exist. ncp then does not search for a project .mcp.json, and runs the servers of this file with no trust question' -r -F
    complete -c ncp -n "__fish_ncp_using_subcommand server; and not __fish_seen_subcommand_from add remove list show" -s v -l verbose -d 'Show log lines on stderr: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change'
    complete -c ncp -n "__fish_ncp_using_subcommand server; and not __fish_seen_subcommand_from add remove list show" -l timings -d 'Show the time of each step on stderr: CPU, configuration, credentials, server start, DNS, connect and each response'
    complete -c ncp -n "__fish_ncp_using_subcommand server; and not __fish_seen_subcommand_from add remove list show" -s q -l quiet -d 'Show no hints, no warnings and no progress on stderr. ncp still shows errors'
    complete -c ncp -n "__fish_ncp_using_subcommand server; and not __fish_seen_subcommand_from add remove list show" -l no-cache -d 'Do not read or write the cache: the protocol version and the tool list of each server'
    complete -c ncp -n "__fish_ncp_using_subcommand server; and not __fish_seen_subcommand_from add remove list show" -l refresh -d 'Do not read the cache, but write it: ncp finds the protocol version and gets the tool list again. --no-cache has priority over --refresh'
    complete -c ncp -n "__fish_ncp_using_subcommand server; and not __fish_seen_subcommand_from add remove list show" -s h -l help -d 'Print help (see more with \'--help\')'
    complete -c ncp -n "__fish_ncp_using_subcommand server; and not __fish_seen_subcommand_from add remove list show" -s V -l version -d 'Print version'
    complete -c ncp -n "__fish_ncp_using_subcommand server; and not __fish_seen_subcommand_from add remove list show" -f -a "add" -d 'Add a server to the project .mcp.json or the global file'
    complete -c ncp -n "__fish_ncp_using_subcommand server; and not __fish_seen_subcommand_from add remove list show" -f -a "remove" -d 'Remove a server from the file that gives it'
    complete -c ncp -n "__fish_ncp_using_subcommand server; and not __fish_seen_subcommand_from add remove list show" -f -a "list" -d 'List the servers of the global file and of the project file'
    complete -c ncp -n "__fish_ncp_using_subcommand server; and not __fish_seen_subcommand_from add remove list show" -f -a "show" -d 'Show the information and the instructions that the server gives in its handshake'
    complete -c ncp -n "__fish_ncp_using_subcommand server; and __fish_seen_subcommand_from add" -l header -d 'A header of an HTTP server. The value of an Authorization or Proxy-Authorization header must be \'<scheme> ${NAME}\' or \'${NAME}\'' -r
    complete -c ncp -n "__fish_ncp_using_subcommand server; and __fish_seen_subcommand_from add" -l env -d 'An environment variable of a stdio server: NAME writes ${NAME}, and NAME=VALUE writes VALUE with no change' -r
    complete -c ncp -n "__fish_ncp_using_subcommand server; and __fish_seen_subcommand_from add" -l secret -d 'Read the value of ${NAME} from the terminal or stdin, and store it for the destination of the entry' -r
    complete -c ncp -n "__fish_ncp_using_subcommand server; and __fish_seen_subcommand_from add" -l credential-store -d 'The credential store for the value of --secret. Default: native' -r -f -a "native\t'The native store: the macOS Keychain or the Linux Secret Service'
    file\t'The credential file, with mode 0600'"
    complete -c ncp -n "__fish_ncp_using_subcommand server; and __fish_seen_subcommand_from add" -l timeout -d 'The time limit in whole seconds for one request, a login or an input. After a timeout, ncp cancels the request or stops the connection' -r
    complete -c ncp -n "__fish_ncp_using_subcommand server; and __fish_seen_subcommand_from add" -l output-format -d 'The output format. Default: text' -r -f -a "text\t'Text for people'
    json\t'One JSON value on stdout, and JSON Lines on stderr'"
    complete -c ncp -n "__fish_ncp_using_subcommand server; and __fish_seen_subcommand_from add" -l config-file -d 'Use this configuration file in place of the global file: ncp/mcp.json in the XDG configuration directory, by default ~/.config/ncp/mcp.json. The file must exist. ncp then does not search for a project .mcp.json, and runs the servers of this file with no trust question' -r -F
    complete -c ncp -n "__fish_ncp_using_subcommand server; and __fish_seen_subcommand_from add" -l global -d 'Add the entry to the global file'
    complete -c ncp -n "__fish_ncp_using_subcommand server; and __fish_seen_subcommand_from add" -s v -l verbose -d 'Show log lines on stderr: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change'
    complete -c ncp -n "__fish_ncp_using_subcommand server; and __fish_seen_subcommand_from add" -l timings -d 'Show the time of each step on stderr: CPU, configuration, credentials, server start, DNS, connect and each response'
    complete -c ncp -n "__fish_ncp_using_subcommand server; and __fish_seen_subcommand_from add" -s q -l quiet -d 'Show no hints, no warnings and no progress on stderr. ncp still shows errors'
    complete -c ncp -n "__fish_ncp_using_subcommand server; and __fish_seen_subcommand_from add" -l no-cache -d 'Do not read or write the cache: the protocol version and the tool list of each server'
    complete -c ncp -n "__fish_ncp_using_subcommand server; and __fish_seen_subcommand_from add" -l refresh -d 'Do not read the cache, but write it: ncp finds the protocol version and gets the tool list again. --no-cache has priority over --refresh'
    complete -c ncp -n "__fish_ncp_using_subcommand server; and __fish_seen_subcommand_from add" -s h -l help -d 'Print help (see more with \'--help\')'
    complete -c ncp -n "__fish_ncp_using_subcommand server; and __fish_seen_subcommand_from add" -s V -l version -d 'Print version'
    complete -c ncp -n "__fish_ncp_using_subcommand server; and __fish_seen_subcommand_from remove" -l timeout -d 'The time limit in whole seconds for one request, a login or an input. After a timeout, ncp cancels the request or stops the connection' -r
    complete -c ncp -n "__fish_ncp_using_subcommand server; and __fish_seen_subcommand_from remove" -l output-format -d 'The output format. Default: text' -r -f -a "text\t'Text for people'
    json\t'One JSON value on stdout, and JSON Lines on stderr'"
    complete -c ncp -n "__fish_ncp_using_subcommand server; and __fish_seen_subcommand_from remove" -l config-file -d 'Use this configuration file in place of the global file: ncp/mcp.json in the XDG configuration directory, by default ~/.config/ncp/mcp.json. The file must exist. ncp then does not search for a project .mcp.json, and runs the servers of this file with no trust question' -r -F
    complete -c ncp -n "__fish_ncp_using_subcommand server; and __fish_seen_subcommand_from remove" -l global -d 'Remove the entry from the global file'
    complete -c ncp -n "__fish_ncp_using_subcommand server; and __fish_seen_subcommand_from remove" -s v -l verbose -d 'Show log lines on stderr: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change'
    complete -c ncp -n "__fish_ncp_using_subcommand server; and __fish_seen_subcommand_from remove" -l timings -d 'Show the time of each step on stderr: CPU, configuration, credentials, server start, DNS, connect and each response'
    complete -c ncp -n "__fish_ncp_using_subcommand server; and __fish_seen_subcommand_from remove" -s q -l quiet -d 'Show no hints, no warnings and no progress on stderr. ncp still shows errors'
    complete -c ncp -n "__fish_ncp_using_subcommand server; and __fish_seen_subcommand_from remove" -l no-cache -d 'Do not read or write the cache: the protocol version and the tool list of each server'
    complete -c ncp -n "__fish_ncp_using_subcommand server; and __fish_seen_subcommand_from remove" -l refresh -d 'Do not read the cache, but write it: ncp finds the protocol version and gets the tool list again. --no-cache has priority over --refresh'
    complete -c ncp -n "__fish_ncp_using_subcommand server; and __fish_seen_subcommand_from remove" -s h -l help -d 'Print help (see more with \'--help\')'
    complete -c ncp -n "__fish_ncp_using_subcommand server; and __fish_seen_subcommand_from remove" -s V -l version -d 'Print version'
    complete -c ncp -n "__fish_ncp_using_subcommand server; and __fish_seen_subcommand_from list" -l timeout -d 'The time limit in whole seconds for one request, a login or an input. After a timeout, ncp cancels the request or stops the connection' -r
    complete -c ncp -n "__fish_ncp_using_subcommand server; and __fish_seen_subcommand_from list" -l output-format -d 'The output format. Default: text' -r -f -a "text\t'Text for people'
    json\t'One JSON value on stdout, and JSON Lines on stderr'"
    complete -c ncp -n "__fish_ncp_using_subcommand server; and __fish_seen_subcommand_from list" -l config-file -d 'Use this configuration file in place of the global file: ncp/mcp.json in the XDG configuration directory, by default ~/.config/ncp/mcp.json. The file must exist. ncp then does not search for a project .mcp.json, and runs the servers of this file with no trust question' -r -F
    complete -c ncp -n "__fish_ncp_using_subcommand server; and __fish_seen_subcommand_from list" -s v -l verbose -d 'Show log lines on stderr: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change'
    complete -c ncp -n "__fish_ncp_using_subcommand server; and __fish_seen_subcommand_from list" -l timings -d 'Show the time of each step on stderr: CPU, configuration, credentials, server start, DNS, connect and each response'
    complete -c ncp -n "__fish_ncp_using_subcommand server; and __fish_seen_subcommand_from list" -s q -l quiet -d 'Show no hints, no warnings and no progress on stderr. ncp still shows errors'
    complete -c ncp -n "__fish_ncp_using_subcommand server; and __fish_seen_subcommand_from list" -l no-cache -d 'Do not read or write the cache: the protocol version and the tool list of each server'
    complete -c ncp -n "__fish_ncp_using_subcommand server; and __fish_seen_subcommand_from list" -l refresh -d 'Do not read the cache, but write it: ncp finds the protocol version and gets the tool list again. --no-cache has priority over --refresh'
    complete -c ncp -n "__fish_ncp_using_subcommand server; and __fish_seen_subcommand_from list" -s h -l help -d 'Print help (see more with \'--help\')'
    complete -c ncp -n "__fish_ncp_using_subcommand server; and __fish_seen_subcommand_from list" -s V -l version -d 'Print version'
    complete -c ncp -n "__fish_ncp_using_subcommand server; and __fish_seen_subcommand_from show" -l timeout -d 'The time limit in whole seconds for one request, a login or an input. After a timeout, ncp cancels the request or stops the connection' -r
    complete -c ncp -n "__fish_ncp_using_subcommand server; and __fish_seen_subcommand_from show" -l output-format -d 'The output format. Default: text' -r -f -a "text\t'Text for people'
    json\t'One JSON value on stdout, and JSON Lines on stderr'"
    complete -c ncp -n "__fish_ncp_using_subcommand server; and __fish_seen_subcommand_from show" -l config-file -d 'Use this configuration file in place of the global file: ncp/mcp.json in the XDG configuration directory, by default ~/.config/ncp/mcp.json. The file must exist. ncp then does not search for a project .mcp.json, and runs the servers of this file with no trust question' -r -F
    complete -c ncp -n "__fish_ncp_using_subcommand server; and __fish_seen_subcommand_from show" -s v -l verbose -d 'Show log lines on stderr: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change'
    complete -c ncp -n "__fish_ncp_using_subcommand server; and __fish_seen_subcommand_from show" -l timings -d 'Show the time of each step on stderr: CPU, configuration, credentials, server start, DNS, connect and each response'
    complete -c ncp -n "__fish_ncp_using_subcommand server; and __fish_seen_subcommand_from show" -s q -l quiet -d 'Show no hints, no warnings and no progress on stderr. ncp still shows errors'
    complete -c ncp -n "__fish_ncp_using_subcommand server; and __fish_seen_subcommand_from show" -l no-cache -d 'Do not read or write the cache: the protocol version and the tool list of each server'
    complete -c ncp -n "__fish_ncp_using_subcommand server; and __fish_seen_subcommand_from show" -l refresh -d 'Do not read the cache, but write it: ncp finds the protocol version and gets the tool list again. --no-cache has priority over --refresh'
    complete -c ncp -n "__fish_ncp_using_subcommand server; and __fish_seen_subcommand_from show" -s h -l help -d 'Print help (see more with \'--help\')'
    complete -c ncp -n "__fish_ncp_using_subcommand server; and __fish_seen_subcommand_from show" -s V -l version -d 'Print version'
    complete -c ncp -n "__fish_ncp_using_subcommand tool; and not __fish_seen_subcommand_from list show call" -l timeout -d 'The time limit in whole seconds for one request, a login or an input. After a timeout, ncp cancels the request or stops the connection' -r
    complete -c ncp -n "__fish_ncp_using_subcommand tool; and not __fish_seen_subcommand_from list show call" -l output-format -d 'The output format. Default: text' -r -f -a "text\t'Text for people'
    json\t'One JSON value on stdout, and JSON Lines on stderr'"
    complete -c ncp -n "__fish_ncp_using_subcommand tool; and not __fish_seen_subcommand_from list show call" -l config-file -d 'Use this configuration file in place of the global file: ncp/mcp.json in the XDG configuration directory, by default ~/.config/ncp/mcp.json. The file must exist. ncp then does not search for a project .mcp.json, and runs the servers of this file with no trust question' -r -F
    complete -c ncp -n "__fish_ncp_using_subcommand tool; and not __fish_seen_subcommand_from list show call" -s v -l verbose -d 'Show log lines on stderr: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change'
    complete -c ncp -n "__fish_ncp_using_subcommand tool; and not __fish_seen_subcommand_from list show call" -l timings -d 'Show the time of each step on stderr: CPU, configuration, credentials, server start, DNS, connect and each response'
    complete -c ncp -n "__fish_ncp_using_subcommand tool; and not __fish_seen_subcommand_from list show call" -s q -l quiet -d 'Show no hints, no warnings and no progress on stderr. ncp still shows errors'
    complete -c ncp -n "__fish_ncp_using_subcommand tool; and not __fish_seen_subcommand_from list show call" -l no-cache -d 'Do not read or write the cache: the protocol version and the tool list of each server'
    complete -c ncp -n "__fish_ncp_using_subcommand tool; and not __fish_seen_subcommand_from list show call" -l refresh -d 'Do not read the cache, but write it: ncp finds the protocol version and gets the tool list again. --no-cache has priority over --refresh'
    complete -c ncp -n "__fish_ncp_using_subcommand tool; and not __fish_seen_subcommand_from list show call" -s h -l help -d 'Print help (see more with \'--help\')'
    complete -c ncp -n "__fish_ncp_using_subcommand tool; and not __fish_seen_subcommand_from list show call" -s V -l version -d 'Print version'
    complete -c ncp -n "__fish_ncp_using_subcommand tool; and not __fish_seen_subcommand_from list show call" -f -a "list" -d 'List the tools of a server'
    complete -c ncp -n "__fish_ncp_using_subcommand tool; and not __fish_seen_subcommand_from list show call" -f -a "show" -d 'Show one tool: its arguments, output and an example call'
    complete -c ncp -n "__fish_ncp_using_subcommand tool; and not __fish_seen_subcommand_from list show call" -f -a "call" -d 'Call a tool'
    complete -c ncp -n "__fish_ncp_using_subcommand tool; and __fish_seen_subcommand_from list" -l timeout -d 'The time limit in whole seconds for one request, a login or an input. After a timeout, ncp cancels the request or stops the connection' -r
    complete -c ncp -n "__fish_ncp_using_subcommand tool; and __fish_seen_subcommand_from list" -l output-format -d 'The output format. Default: text' -r -f -a "text\t'Text for people'
    json\t'One JSON value on stdout, and JSON Lines on stderr'"
    complete -c ncp -n "__fish_ncp_using_subcommand tool; and __fish_seen_subcommand_from list" -l config-file -d 'Use this configuration file in place of the global file: ncp/mcp.json in the XDG configuration directory, by default ~/.config/ncp/mcp.json. The file must exist. ncp then does not search for a project .mcp.json, and runs the servers of this file with no trust question' -r -F
    complete -c ncp -n "__fish_ncp_using_subcommand tool; and __fish_seen_subcommand_from list" -s v -l verbose -d 'Show log lines on stderr: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change'
    complete -c ncp -n "__fish_ncp_using_subcommand tool; and __fish_seen_subcommand_from list" -l timings -d 'Show the time of each step on stderr: CPU, configuration, credentials, server start, DNS, connect and each response'
    complete -c ncp -n "__fish_ncp_using_subcommand tool; and __fish_seen_subcommand_from list" -s q -l quiet -d 'Show no hints, no warnings and no progress on stderr. ncp still shows errors'
    complete -c ncp -n "__fish_ncp_using_subcommand tool; and __fish_seen_subcommand_from list" -l no-cache -d 'Do not read or write the cache: the protocol version and the tool list of each server'
    complete -c ncp -n "__fish_ncp_using_subcommand tool; and __fish_seen_subcommand_from list" -l refresh -d 'Do not read the cache, but write it: ncp finds the protocol version and gets the tool list again. --no-cache has priority over --refresh'
    complete -c ncp -n "__fish_ncp_using_subcommand tool; and __fish_seen_subcommand_from list" -s h -l help -d 'Print help (see more with \'--help\')'
    complete -c ncp -n "__fish_ncp_using_subcommand tool; and __fish_seen_subcommand_from list" -s V -l version -d 'Print version'
    complete -c ncp -n "__fish_ncp_using_subcommand tool; and __fish_seen_subcommand_from show" -l timeout -d 'The time limit in whole seconds for one request, a login or an input. After a timeout, ncp cancels the request or stops the connection' -r
    complete -c ncp -n "__fish_ncp_using_subcommand tool; and __fish_seen_subcommand_from show" -l output-format -d 'The output format. Default: text' -r -f -a "text\t'Text for people'
    json\t'One JSON value on stdout, and JSON Lines on stderr'"
    complete -c ncp -n "__fish_ncp_using_subcommand tool; and __fish_seen_subcommand_from show" -l config-file -d 'Use this configuration file in place of the global file: ncp/mcp.json in the XDG configuration directory, by default ~/.config/ncp/mcp.json. The file must exist. ncp then does not search for a project .mcp.json, and runs the servers of this file with no trust question' -r -F
    complete -c ncp -n "__fish_ncp_using_subcommand tool; and __fish_seen_subcommand_from show" -s v -l verbose -d 'Show log lines on stderr: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change'
    complete -c ncp -n "__fish_ncp_using_subcommand tool; and __fish_seen_subcommand_from show" -l timings -d 'Show the time of each step on stderr: CPU, configuration, credentials, server start, DNS, connect and each response'
    complete -c ncp -n "__fish_ncp_using_subcommand tool; and __fish_seen_subcommand_from show" -s q -l quiet -d 'Show no hints, no warnings and no progress on stderr. ncp still shows errors'
    complete -c ncp -n "__fish_ncp_using_subcommand tool; and __fish_seen_subcommand_from show" -l no-cache -d 'Do not read or write the cache: the protocol version and the tool list of each server'
    complete -c ncp -n "__fish_ncp_using_subcommand tool; and __fish_seen_subcommand_from show" -l refresh -d 'Do not read the cache, but write it: ncp finds the protocol version and gets the tool list again. --no-cache has priority over --refresh'
    complete -c ncp -n "__fish_ncp_using_subcommand tool; and __fish_seen_subcommand_from show" -s h -l help -d 'Print help (see more with \'--help\')'
    complete -c ncp -n "__fish_ncp_using_subcommand tool; and __fish_seen_subcommand_from show" -s V -l version -d 'Print version'
    complete -c ncp -n "__fish_ncp_using_subcommand tool; and __fish_seen_subcommand_from call" -l timeout -d 'The time limit in whole seconds for one request, a login or an input. After a timeout, ncp cancels the request or stops the connection' -r
    complete -c ncp -n "__fish_ncp_using_subcommand tool; and __fish_seen_subcommand_from call" -l output-format -d 'The output format. Default: text' -r -f -a "text\t'Text for people'
    json\t'One JSON value on stdout, and JSON Lines on stderr'"
    complete -c ncp -n "__fish_ncp_using_subcommand tool; and __fish_seen_subcommand_from call" -l config-file -d 'Use this configuration file in place of the global file: ncp/mcp.json in the XDG configuration directory, by default ~/.config/ncp/mcp.json. The file must exist. ncp then does not search for a project .mcp.json, and runs the servers of this file with no trust question' -r -F
    complete -c ncp -n "__fish_ncp_using_subcommand tool; and __fish_seen_subcommand_from call" -l no-validate -d 'Do not check the arguments against the input schema of the tool'
    complete -c ncp -n "__fish_ncp_using_subcommand tool; and __fish_seen_subcommand_from call" -s h -l help -d 'Show the tool, as ncp tool show does. With no tool, show this help'
    complete -c ncp -n "__fish_ncp_using_subcommand tool; and __fish_seen_subcommand_from call" -s v -l verbose -d 'Show log lines on stderr: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change'
    complete -c ncp -n "__fish_ncp_using_subcommand tool; and __fish_seen_subcommand_from call" -l timings -d 'Show the time of each step on stderr: CPU, configuration, credentials, server start, DNS, connect and each response'
    complete -c ncp -n "__fish_ncp_using_subcommand tool; and __fish_seen_subcommand_from call" -s q -l quiet -d 'Show no hints, no warnings and no progress on stderr. ncp still shows errors'
    complete -c ncp -n "__fish_ncp_using_subcommand tool; and __fish_seen_subcommand_from call" -l no-cache -d 'Do not read or write the cache: the protocol version and the tool list of each server'
    complete -c ncp -n "__fish_ncp_using_subcommand tool; and __fish_seen_subcommand_from call" -l refresh -d 'Do not read the cache, but write it: ncp finds the protocol version and gets the tool list again. --no-cache has priority over --refresh'
    complete -c ncp -n "__fish_ncp_using_subcommand tool; and __fish_seen_subcommand_from call" -s V -l version -d 'Print version'
    complete -c ncp -n "__fish_ncp_using_subcommand resource; and not __fish_seen_subcommand_from list read" -l timeout -d 'The time limit in whole seconds for one request, a login or an input. After a timeout, ncp cancels the request or stops the connection' -r
    complete -c ncp -n "__fish_ncp_using_subcommand resource; and not __fish_seen_subcommand_from list read" -l output-format -d 'The output format. Default: text' -r -f -a "text\t'Text for people'
    json\t'One JSON value on stdout, and JSON Lines on stderr'"
    complete -c ncp -n "__fish_ncp_using_subcommand resource; and not __fish_seen_subcommand_from list read" -l config-file -d 'Use this configuration file in place of the global file: ncp/mcp.json in the XDG configuration directory, by default ~/.config/ncp/mcp.json. The file must exist. ncp then does not search for a project .mcp.json, and runs the servers of this file with no trust question' -r -F
    complete -c ncp -n "__fish_ncp_using_subcommand resource; and not __fish_seen_subcommand_from list read" -s v -l verbose -d 'Show log lines on stderr: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change'
    complete -c ncp -n "__fish_ncp_using_subcommand resource; and not __fish_seen_subcommand_from list read" -l timings -d 'Show the time of each step on stderr: CPU, configuration, credentials, server start, DNS, connect and each response'
    complete -c ncp -n "__fish_ncp_using_subcommand resource; and not __fish_seen_subcommand_from list read" -s q -l quiet -d 'Show no hints, no warnings and no progress on stderr. ncp still shows errors'
    complete -c ncp -n "__fish_ncp_using_subcommand resource; and not __fish_seen_subcommand_from list read" -l no-cache -d 'Do not read or write the cache: the protocol version and the tool list of each server'
    complete -c ncp -n "__fish_ncp_using_subcommand resource; and not __fish_seen_subcommand_from list read" -l refresh -d 'Do not read the cache, but write it: ncp finds the protocol version and gets the tool list again. --no-cache has priority over --refresh'
    complete -c ncp -n "__fish_ncp_using_subcommand resource; and not __fish_seen_subcommand_from list read" -s h -l help -d 'Print help (see more with \'--help\')'
    complete -c ncp -n "__fish_ncp_using_subcommand resource; and not __fish_seen_subcommand_from list read" -s V -l version -d 'Print version'
    complete -c ncp -n "__fish_ncp_using_subcommand resource; and not __fish_seen_subcommand_from list read" -f -a "list" -d 'List the resources and the resource templates of a server'
    complete -c ncp -n "__fish_ncp_using_subcommand resource; and not __fish_seen_subcommand_from list read" -f -a "read" -d 'Read one resource'
    complete -c ncp -n "__fish_ncp_using_subcommand resource; and __fish_seen_subcommand_from list" -l timeout -d 'The time limit in whole seconds for one request, a login or an input. After a timeout, ncp cancels the request or stops the connection' -r
    complete -c ncp -n "__fish_ncp_using_subcommand resource; and __fish_seen_subcommand_from list" -l output-format -d 'The output format. Default: text' -r -f -a "text\t'Text for people'
    json\t'One JSON value on stdout, and JSON Lines on stderr'"
    complete -c ncp -n "__fish_ncp_using_subcommand resource; and __fish_seen_subcommand_from list" -l config-file -d 'Use this configuration file in place of the global file: ncp/mcp.json in the XDG configuration directory, by default ~/.config/ncp/mcp.json. The file must exist. ncp then does not search for a project .mcp.json, and runs the servers of this file with no trust question' -r -F
    complete -c ncp -n "__fish_ncp_using_subcommand resource; and __fish_seen_subcommand_from list" -s v -l verbose -d 'Show log lines on stderr: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change'
    complete -c ncp -n "__fish_ncp_using_subcommand resource; and __fish_seen_subcommand_from list" -l timings -d 'Show the time of each step on stderr: CPU, configuration, credentials, server start, DNS, connect and each response'
    complete -c ncp -n "__fish_ncp_using_subcommand resource; and __fish_seen_subcommand_from list" -s q -l quiet -d 'Show no hints, no warnings and no progress on stderr. ncp still shows errors'
    complete -c ncp -n "__fish_ncp_using_subcommand resource; and __fish_seen_subcommand_from list" -l no-cache -d 'Do not read or write the cache: the protocol version and the tool list of each server'
    complete -c ncp -n "__fish_ncp_using_subcommand resource; and __fish_seen_subcommand_from list" -l refresh -d 'Do not read the cache, but write it: ncp finds the protocol version and gets the tool list again. --no-cache has priority over --refresh'
    complete -c ncp -n "__fish_ncp_using_subcommand resource; and __fish_seen_subcommand_from list" -s h -l help -d 'Print help (see more with \'--help\')'
    complete -c ncp -n "__fish_ncp_using_subcommand resource; and __fish_seen_subcommand_from list" -s V -l version -d 'Print version'
    complete -c ncp -n "__fish_ncp_using_subcommand resource; and __fish_seen_subcommand_from read" -l timeout -d 'The time limit in whole seconds for one request, a login or an input. After a timeout, ncp cancels the request or stops the connection' -r
    complete -c ncp -n "__fish_ncp_using_subcommand resource; and __fish_seen_subcommand_from read" -l output-format -d 'The output format. Default: text' -r -f -a "text\t'Text for people'
    json\t'One JSON value on stdout, and JSON Lines on stderr'"
    complete -c ncp -n "__fish_ncp_using_subcommand resource; and __fish_seen_subcommand_from read" -l config-file -d 'Use this configuration file in place of the global file: ncp/mcp.json in the XDG configuration directory, by default ~/.config/ncp/mcp.json. The file must exist. ncp then does not search for a project .mcp.json, and runs the servers of this file with no trust question' -r -F
    complete -c ncp -n "__fish_ncp_using_subcommand resource; and __fish_seen_subcommand_from read" -s v -l verbose -d 'Show log lines on stderr: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change'
    complete -c ncp -n "__fish_ncp_using_subcommand resource; and __fish_seen_subcommand_from read" -l timings -d 'Show the time of each step on stderr: CPU, configuration, credentials, server start, DNS, connect and each response'
    complete -c ncp -n "__fish_ncp_using_subcommand resource; and __fish_seen_subcommand_from read" -s q -l quiet -d 'Show no hints, no warnings and no progress on stderr. ncp still shows errors'
    complete -c ncp -n "__fish_ncp_using_subcommand resource; and __fish_seen_subcommand_from read" -l no-cache -d 'Do not read or write the cache: the protocol version and the tool list of each server'
    complete -c ncp -n "__fish_ncp_using_subcommand resource; and __fish_seen_subcommand_from read" -l refresh -d 'Do not read the cache, but write it: ncp finds the protocol version and gets the tool list again. --no-cache has priority over --refresh'
    complete -c ncp -n "__fish_ncp_using_subcommand resource; and __fish_seen_subcommand_from read" -s h -l help -d 'Print help (see more with \'--help\')'
    complete -c ncp -n "__fish_ncp_using_subcommand resource; and __fish_seen_subcommand_from read" -s V -l version -d 'Print version'
    complete -c ncp -n "__fish_ncp_using_subcommand prompt; and not __fish_seen_subcommand_from list get" -l timeout -d 'The time limit in whole seconds for one request, a login or an input. After a timeout, ncp cancels the request or stops the connection' -r
    complete -c ncp -n "__fish_ncp_using_subcommand prompt; and not __fish_seen_subcommand_from list get" -l output-format -d 'The output format. Default: text' -r -f -a "text\t'Text for people'
    json\t'One JSON value on stdout, and JSON Lines on stderr'"
    complete -c ncp -n "__fish_ncp_using_subcommand prompt; and not __fish_seen_subcommand_from list get" -l config-file -d 'Use this configuration file in place of the global file: ncp/mcp.json in the XDG configuration directory, by default ~/.config/ncp/mcp.json. The file must exist. ncp then does not search for a project .mcp.json, and runs the servers of this file with no trust question' -r -F
    complete -c ncp -n "__fish_ncp_using_subcommand prompt; and not __fish_seen_subcommand_from list get" -s v -l verbose -d 'Show log lines on stderr: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change'
    complete -c ncp -n "__fish_ncp_using_subcommand prompt; and not __fish_seen_subcommand_from list get" -l timings -d 'Show the time of each step on stderr: CPU, configuration, credentials, server start, DNS, connect and each response'
    complete -c ncp -n "__fish_ncp_using_subcommand prompt; and not __fish_seen_subcommand_from list get" -s q -l quiet -d 'Show no hints, no warnings and no progress on stderr. ncp still shows errors'
    complete -c ncp -n "__fish_ncp_using_subcommand prompt; and not __fish_seen_subcommand_from list get" -l no-cache -d 'Do not read or write the cache: the protocol version and the tool list of each server'
    complete -c ncp -n "__fish_ncp_using_subcommand prompt; and not __fish_seen_subcommand_from list get" -l refresh -d 'Do not read the cache, but write it: ncp finds the protocol version and gets the tool list again. --no-cache has priority over --refresh'
    complete -c ncp -n "__fish_ncp_using_subcommand prompt; and not __fish_seen_subcommand_from list get" -s h -l help -d 'Print help (see more with \'--help\')'
    complete -c ncp -n "__fish_ncp_using_subcommand prompt; and not __fish_seen_subcommand_from list get" -s V -l version -d 'Print version'
    complete -c ncp -n "__fish_ncp_using_subcommand prompt; and not __fish_seen_subcommand_from list get" -f -a "list" -d 'List the prompts of a server'
    complete -c ncp -n "__fish_ncp_using_subcommand prompt; and not __fish_seen_subcommand_from list get" -f -a "get" -d 'Get one prompt'
    complete -c ncp -n "__fish_ncp_using_subcommand prompt; and __fish_seen_subcommand_from list" -l timeout -d 'The time limit in whole seconds for one request, a login or an input. After a timeout, ncp cancels the request or stops the connection' -r
    complete -c ncp -n "__fish_ncp_using_subcommand prompt; and __fish_seen_subcommand_from list" -l output-format -d 'The output format. Default: text' -r -f -a "text\t'Text for people'
    json\t'One JSON value on stdout, and JSON Lines on stderr'"
    complete -c ncp -n "__fish_ncp_using_subcommand prompt; and __fish_seen_subcommand_from list" -l config-file -d 'Use this configuration file in place of the global file: ncp/mcp.json in the XDG configuration directory, by default ~/.config/ncp/mcp.json. The file must exist. ncp then does not search for a project .mcp.json, and runs the servers of this file with no trust question' -r -F
    complete -c ncp -n "__fish_ncp_using_subcommand prompt; and __fish_seen_subcommand_from list" -s v -l verbose -d 'Show log lines on stderr: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change'
    complete -c ncp -n "__fish_ncp_using_subcommand prompt; and __fish_seen_subcommand_from list" -l timings -d 'Show the time of each step on stderr: CPU, configuration, credentials, server start, DNS, connect and each response'
    complete -c ncp -n "__fish_ncp_using_subcommand prompt; and __fish_seen_subcommand_from list" -s q -l quiet -d 'Show no hints, no warnings and no progress on stderr. ncp still shows errors'
    complete -c ncp -n "__fish_ncp_using_subcommand prompt; and __fish_seen_subcommand_from list" -l no-cache -d 'Do not read or write the cache: the protocol version and the tool list of each server'
    complete -c ncp -n "__fish_ncp_using_subcommand prompt; and __fish_seen_subcommand_from list" -l refresh -d 'Do not read the cache, but write it: ncp finds the protocol version and gets the tool list again. --no-cache has priority over --refresh'
    complete -c ncp -n "__fish_ncp_using_subcommand prompt; and __fish_seen_subcommand_from list" -s h -l help -d 'Print help (see more with \'--help\')'
    complete -c ncp -n "__fish_ncp_using_subcommand prompt; and __fish_seen_subcommand_from list" -s V -l version -d 'Print version'
    complete -c ncp -n "__fish_ncp_using_subcommand prompt; and __fish_seen_subcommand_from get" -l timeout -d 'The time limit in whole seconds for one request, a login or an input. After a timeout, ncp cancels the request or stops the connection' -r
    complete -c ncp -n "__fish_ncp_using_subcommand prompt; and __fish_seen_subcommand_from get" -l output-format -d 'The output format. Default: text' -r -f -a "text\t'Text for people'
    json\t'One JSON value on stdout, and JSON Lines on stderr'"
    complete -c ncp -n "__fish_ncp_using_subcommand prompt; and __fish_seen_subcommand_from get" -l config-file -d 'Use this configuration file in place of the global file: ncp/mcp.json in the XDG configuration directory, by default ~/.config/ncp/mcp.json. The file must exist. ncp then does not search for a project .mcp.json, and runs the servers of this file with no trust question' -r -F
    complete -c ncp -n "__fish_ncp_using_subcommand prompt; and __fish_seen_subcommand_from get" -s v -l verbose -d 'Show log lines on stderr: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change'
    complete -c ncp -n "__fish_ncp_using_subcommand prompt; and __fish_seen_subcommand_from get" -l timings -d 'Show the time of each step on stderr: CPU, configuration, credentials, server start, DNS, connect and each response'
    complete -c ncp -n "__fish_ncp_using_subcommand prompt; and __fish_seen_subcommand_from get" -s q -l quiet -d 'Show no hints, no warnings and no progress on stderr. ncp still shows errors'
    complete -c ncp -n "__fish_ncp_using_subcommand prompt; and __fish_seen_subcommand_from get" -l no-cache -d 'Do not read or write the cache: the protocol version and the tool list of each server'
    complete -c ncp -n "__fish_ncp_using_subcommand prompt; and __fish_seen_subcommand_from get" -l refresh -d 'Do not read the cache, but write it: ncp finds the protocol version and gets the tool list again. --no-cache has priority over --refresh'
    complete -c ncp -n "__fish_ncp_using_subcommand prompt; and __fish_seen_subcommand_from get" -s h -l help -d 'Print help (see more with \'--help\')'
    complete -c ncp -n "__fish_ncp_using_subcommand prompt; and __fish_seen_subcommand_from get" -s V -l version -d 'Print version'
    complete -c ncp -n "__fish_ncp_using_subcommand auth; and not __fish_seen_subcommand_from login logout status" -l timeout -d 'The time limit in whole seconds for one request, a login or an input. After a timeout, ncp cancels the request or stops the connection' -r
    complete -c ncp -n "__fish_ncp_using_subcommand auth; and not __fish_seen_subcommand_from login logout status" -l output-format -d 'The output format. Default: text' -r -f -a "text\t'Text for people'
    json\t'One JSON value on stdout, and JSON Lines on stderr'"
    complete -c ncp -n "__fish_ncp_using_subcommand auth; and not __fish_seen_subcommand_from login logout status" -l config-file -d 'Use this configuration file in place of the global file: ncp/mcp.json in the XDG configuration directory, by default ~/.config/ncp/mcp.json. The file must exist. ncp then does not search for a project .mcp.json, and runs the servers of this file with no trust question' -r -F
    complete -c ncp -n "__fish_ncp_using_subcommand auth; and not __fish_seen_subcommand_from login logout status" -s v -l verbose -d 'Show log lines on stderr: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change'
    complete -c ncp -n "__fish_ncp_using_subcommand auth; and not __fish_seen_subcommand_from login logout status" -l timings -d 'Show the time of each step on stderr: CPU, configuration, credentials, server start, DNS, connect and each response'
    complete -c ncp -n "__fish_ncp_using_subcommand auth; and not __fish_seen_subcommand_from login logout status" -s q -l quiet -d 'Show no hints, no warnings and no progress on stderr. ncp still shows errors'
    complete -c ncp -n "__fish_ncp_using_subcommand auth; and not __fish_seen_subcommand_from login logout status" -l no-cache -d 'Do not read or write the cache: the protocol version and the tool list of each server'
    complete -c ncp -n "__fish_ncp_using_subcommand auth; and not __fish_seen_subcommand_from login logout status" -l refresh -d 'Do not read the cache, but write it: ncp finds the protocol version and gets the tool list again. --no-cache has priority over --refresh'
    complete -c ncp -n "__fish_ncp_using_subcommand auth; and not __fish_seen_subcommand_from login logout status" -s h -l help -d 'Print help (see more with \'--help\')'
    complete -c ncp -n "__fish_ncp_using_subcommand auth; and not __fish_seen_subcommand_from login logout status" -s V -l version -d 'Print version'
    complete -c ncp -n "__fish_ncp_using_subcommand auth; and not __fish_seen_subcommand_from login logout status" -f -a "login" -d 'Log in to an HTTP server with OAuth and store the tokens'
    complete -c ncp -n "__fish_ncp_using_subcommand auth; and not __fish_seen_subcommand_from login logout status" -f -a "logout" -d 'Delete the OAuth tokens of a server from all credential stores'
    complete -c ncp -n "__fish_ncp_using_subcommand auth; and not __fish_seen_subcommand_from login logout status" -f -a "status" -d 'Show the OAuth login of each server, or of one server'
    complete -c ncp -n "__fish_ncp_using_subcommand auth; and __fish_seen_subcommand_from login" -l credential-store -d 'The credential store for the tokens' -r -f -a "native\t'The native store: the macOS Keychain or the Linux Secret Service'
    file\t'The credential file, with mode 0600'"
    complete -c ncp -n "__fish_ncp_using_subcommand auth; and __fish_seen_subcommand_from login" -l timeout -d 'The time limit in whole seconds for one request, a login or an input. After a timeout, ncp cancels the request or stops the connection' -r
    complete -c ncp -n "__fish_ncp_using_subcommand auth; and __fish_seen_subcommand_from login" -l output-format -d 'The output format. Default: text' -r -f -a "text\t'Text for people'
    json\t'One JSON value on stdout, and JSON Lines on stderr'"
    complete -c ncp -n "__fish_ncp_using_subcommand auth; and __fish_seen_subcommand_from login" -l config-file -d 'Use this configuration file in place of the global file: ncp/mcp.json in the XDG configuration directory, by default ~/.config/ncp/mcp.json. The file must exist. ncp then does not search for a project .mcp.json, and runs the servers of this file with no trust question' -r -F
    complete -c ncp -n "__fish_ncp_using_subcommand auth; and __fish_seen_subcommand_from login" -l no-browser -d 'Do not open a browser: show the URL, and read the redirect URL from stdin'
    complete -c ncp -n "__fish_ncp_using_subcommand auth; and __fish_seen_subcommand_from login" -s v -l verbose -d 'Show log lines on stderr: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change'
    complete -c ncp -n "__fish_ncp_using_subcommand auth; and __fish_seen_subcommand_from login" -l timings -d 'Show the time of each step on stderr: CPU, configuration, credentials, server start, DNS, connect and each response'
    complete -c ncp -n "__fish_ncp_using_subcommand auth; and __fish_seen_subcommand_from login" -s q -l quiet -d 'Show no hints, no warnings and no progress on stderr. ncp still shows errors'
    complete -c ncp -n "__fish_ncp_using_subcommand auth; and __fish_seen_subcommand_from login" -l no-cache -d 'Do not read or write the cache: the protocol version and the tool list of each server'
    complete -c ncp -n "__fish_ncp_using_subcommand auth; and __fish_seen_subcommand_from login" -l refresh -d 'Do not read the cache, but write it: ncp finds the protocol version and gets the tool list again. --no-cache has priority over --refresh'
    complete -c ncp -n "__fish_ncp_using_subcommand auth; and __fish_seen_subcommand_from login" -s h -l help -d 'Print help (see more with \'--help\')'
    complete -c ncp -n "__fish_ncp_using_subcommand auth; and __fish_seen_subcommand_from login" -s V -l version -d 'Print version'
    complete -c ncp -n "__fish_ncp_using_subcommand auth; and __fish_seen_subcommand_from logout" -l timeout -d 'The time limit in whole seconds for one request, a login or an input. After a timeout, ncp cancels the request or stops the connection' -r
    complete -c ncp -n "__fish_ncp_using_subcommand auth; and __fish_seen_subcommand_from logout" -l output-format -d 'The output format. Default: text' -r -f -a "text\t'Text for people'
    json\t'One JSON value on stdout, and JSON Lines on stderr'"
    complete -c ncp -n "__fish_ncp_using_subcommand auth; and __fish_seen_subcommand_from logout" -l config-file -d 'Use this configuration file in place of the global file: ncp/mcp.json in the XDG configuration directory, by default ~/.config/ncp/mcp.json. The file must exist. ncp then does not search for a project .mcp.json, and runs the servers of this file with no trust question' -r -F
    complete -c ncp -n "__fish_ncp_using_subcommand auth; and __fish_seen_subcommand_from logout" -s v -l verbose -d 'Show log lines on stderr: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change'
    complete -c ncp -n "__fish_ncp_using_subcommand auth; and __fish_seen_subcommand_from logout" -l timings -d 'Show the time of each step on stderr: CPU, configuration, credentials, server start, DNS, connect and each response'
    complete -c ncp -n "__fish_ncp_using_subcommand auth; and __fish_seen_subcommand_from logout" -s q -l quiet -d 'Show no hints, no warnings and no progress on stderr. ncp still shows errors'
    complete -c ncp -n "__fish_ncp_using_subcommand auth; and __fish_seen_subcommand_from logout" -l no-cache -d 'Do not read or write the cache: the protocol version and the tool list of each server'
    complete -c ncp -n "__fish_ncp_using_subcommand auth; and __fish_seen_subcommand_from logout" -l refresh -d 'Do not read the cache, but write it: ncp finds the protocol version and gets the tool list again. --no-cache has priority over --refresh'
    complete -c ncp -n "__fish_ncp_using_subcommand auth; and __fish_seen_subcommand_from logout" -s h -l help -d 'Print help (see more with \'--help\')'
    complete -c ncp -n "__fish_ncp_using_subcommand auth; and __fish_seen_subcommand_from logout" -s V -l version -d 'Print version'
    complete -c ncp -n "__fish_ncp_using_subcommand auth; and __fish_seen_subcommand_from status" -l timeout -d 'The time limit in whole seconds for one request, a login or an input. After a timeout, ncp cancels the request or stops the connection' -r
    complete -c ncp -n "__fish_ncp_using_subcommand auth; and __fish_seen_subcommand_from status" -l output-format -d 'The output format. Default: text' -r -f -a "text\t'Text for people'
    json\t'One JSON value on stdout, and JSON Lines on stderr'"
    complete -c ncp -n "__fish_ncp_using_subcommand auth; and __fish_seen_subcommand_from status" -l config-file -d 'Use this configuration file in place of the global file: ncp/mcp.json in the XDG configuration directory, by default ~/.config/ncp/mcp.json. The file must exist. ncp then does not search for a project .mcp.json, and runs the servers of this file with no trust question' -r -F
    complete -c ncp -n "__fish_ncp_using_subcommand auth; and __fish_seen_subcommand_from status" -s v -l verbose -d 'Show log lines on stderr: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change'
    complete -c ncp -n "__fish_ncp_using_subcommand auth; and __fish_seen_subcommand_from status" -l timings -d 'Show the time of each step on stderr: CPU, configuration, credentials, server start, DNS, connect and each response'
    complete -c ncp -n "__fish_ncp_using_subcommand auth; and __fish_seen_subcommand_from status" -s q -l quiet -d 'Show no hints, no warnings and no progress on stderr. ncp still shows errors'
    complete -c ncp -n "__fish_ncp_using_subcommand auth; and __fish_seen_subcommand_from status" -l no-cache -d 'Do not read or write the cache: the protocol version and the tool list of each server'
    complete -c ncp -n "__fish_ncp_using_subcommand auth; and __fish_seen_subcommand_from status" -l refresh -d 'Do not read the cache, but write it: ncp finds the protocol version and gets the tool list again. --no-cache has priority over --refresh'
    complete -c ncp -n "__fish_ncp_using_subcommand auth; and __fish_seen_subcommand_from status" -s h -l help -d 'Print help (see more with \'--help\')'
    complete -c ncp -n "__fish_ncp_using_subcommand auth; and __fish_seen_subcommand_from status" -s V -l version -d 'Print version'
    complete -c ncp -n "__fish_ncp_using_subcommand secret; and not __fish_seen_subcommand_from set list delete" -l timeout -d 'The time limit in whole seconds for one request, a login or an input. After a timeout, ncp cancels the request or stops the connection' -r
    complete -c ncp -n "__fish_ncp_using_subcommand secret; and not __fish_seen_subcommand_from set list delete" -l output-format -d 'The output format. Default: text' -r -f -a "text\t'Text for people'
    json\t'One JSON value on stdout, and JSON Lines on stderr'"
    complete -c ncp -n "__fish_ncp_using_subcommand secret; and not __fish_seen_subcommand_from set list delete" -l config-file -d 'Use this configuration file in place of the global file: ncp/mcp.json in the XDG configuration directory, by default ~/.config/ncp/mcp.json. The file must exist. ncp then does not search for a project .mcp.json, and runs the servers of this file with no trust question' -r -F
    complete -c ncp -n "__fish_ncp_using_subcommand secret; and not __fish_seen_subcommand_from set list delete" -s v -l verbose -d 'Show log lines on stderr: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change'
    complete -c ncp -n "__fish_ncp_using_subcommand secret; and not __fish_seen_subcommand_from set list delete" -l timings -d 'Show the time of each step on stderr: CPU, configuration, credentials, server start, DNS, connect and each response'
    complete -c ncp -n "__fish_ncp_using_subcommand secret; and not __fish_seen_subcommand_from set list delete" -s q -l quiet -d 'Show no hints, no warnings and no progress on stderr. ncp still shows errors'
    complete -c ncp -n "__fish_ncp_using_subcommand secret; and not __fish_seen_subcommand_from set list delete" -l no-cache -d 'Do not read or write the cache: the protocol version and the tool list of each server'
    complete -c ncp -n "__fish_ncp_using_subcommand secret; and not __fish_seen_subcommand_from set list delete" -l refresh -d 'Do not read the cache, but write it: ncp finds the protocol version and gets the tool list again. --no-cache has priority over --refresh'
    complete -c ncp -n "__fish_ncp_using_subcommand secret; and not __fish_seen_subcommand_from set list delete" -s h -l help -d 'Print help (see more with \'--help\')'
    complete -c ncp -n "__fish_ncp_using_subcommand secret; and not __fish_seen_subcommand_from set list delete" -s V -l version -d 'Print version'
    complete -c ncp -n "__fish_ncp_using_subcommand secret; and not __fish_seen_subcommand_from set list delete" -f -a "set" -d 'Store a value for ${NAME}'
    complete -c ncp -n "__fish_ncp_using_subcommand secret; and not __fish_seen_subcommand_from set list delete" -f -a "list" -d 'List the stored secrets'
    complete -c ncp -n "__fish_ncp_using_subcommand secret; and not __fish_seen_subcommand_from set list delete" -f -a "delete" -d 'Delete a stored secret from all credential stores'
    complete -c ncp -n "__fish_ncp_using_subcommand secret; and __fish_seen_subcommand_from set" -l server -d 'Use only the destination of this server' -r
    complete -c ncp -n "__fish_ncp_using_subcommand secret; and __fish_seen_subcommand_from set" -l credential-store -d 'The credential store for the value' -r -f -a "native\t'The native store: the macOS Keychain or the Linux Secret Service'
    file\t'The credential file, with mode 0600'"
    complete -c ncp -n "__fish_ncp_using_subcommand secret; and __fish_seen_subcommand_from set" -l timeout -d 'The time limit in whole seconds for one request, a login or an input. After a timeout, ncp cancels the request or stops the connection' -r
    complete -c ncp -n "__fish_ncp_using_subcommand secret; and __fish_seen_subcommand_from set" -l output-format -d 'The output format. Default: text' -r -f -a "text\t'Text for people'
    json\t'One JSON value on stdout, and JSON Lines on stderr'"
    complete -c ncp -n "__fish_ncp_using_subcommand secret; and __fish_seen_subcommand_from set" -l config-file -d 'Use this configuration file in place of the global file: ncp/mcp.json in the XDG configuration directory, by default ~/.config/ncp/mcp.json. The file must exist. ncp then does not search for a project .mcp.json, and runs the servers of this file with no trust question' -r -F
    complete -c ncp -n "__fish_ncp_using_subcommand secret; and __fish_seen_subcommand_from set" -s v -l verbose -d 'Show log lines on stderr: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change'
    complete -c ncp -n "__fish_ncp_using_subcommand secret; and __fish_seen_subcommand_from set" -l timings -d 'Show the time of each step on stderr: CPU, configuration, credentials, server start, DNS, connect and each response'
    complete -c ncp -n "__fish_ncp_using_subcommand secret; and __fish_seen_subcommand_from set" -s q -l quiet -d 'Show no hints, no warnings and no progress on stderr. ncp still shows errors'
    complete -c ncp -n "__fish_ncp_using_subcommand secret; and __fish_seen_subcommand_from set" -l no-cache -d 'Do not read or write the cache: the protocol version and the tool list of each server'
    complete -c ncp -n "__fish_ncp_using_subcommand secret; and __fish_seen_subcommand_from set" -l refresh -d 'Do not read the cache, but write it: ncp finds the protocol version and gets the tool list again. --no-cache has priority over --refresh'
    complete -c ncp -n "__fish_ncp_using_subcommand secret; and __fish_seen_subcommand_from set" -s h -l help -d 'Print help (see more with \'--help\')'
    complete -c ncp -n "__fish_ncp_using_subcommand secret; and __fish_seen_subcommand_from set" -s V -l version -d 'Print version'
    complete -c ncp -n "__fish_ncp_using_subcommand secret; and __fish_seen_subcommand_from list" -l timeout -d 'The time limit in whole seconds for one request, a login or an input. After a timeout, ncp cancels the request or stops the connection' -r
    complete -c ncp -n "__fish_ncp_using_subcommand secret; and __fish_seen_subcommand_from list" -l output-format -d 'The output format. Default: text' -r -f -a "text\t'Text for people'
    json\t'One JSON value on stdout, and JSON Lines on stderr'"
    complete -c ncp -n "__fish_ncp_using_subcommand secret; and __fish_seen_subcommand_from list" -l config-file -d 'Use this configuration file in place of the global file: ncp/mcp.json in the XDG configuration directory, by default ~/.config/ncp/mcp.json. The file must exist. ncp then does not search for a project .mcp.json, and runs the servers of this file with no trust question' -r -F
    complete -c ncp -n "__fish_ncp_using_subcommand secret; and __fish_seen_subcommand_from list" -s v -l verbose -d 'Show log lines on stderr: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change'
    complete -c ncp -n "__fish_ncp_using_subcommand secret; and __fish_seen_subcommand_from list" -l timings -d 'Show the time of each step on stderr: CPU, configuration, credentials, server start, DNS, connect and each response'
    complete -c ncp -n "__fish_ncp_using_subcommand secret; and __fish_seen_subcommand_from list" -s q -l quiet -d 'Show no hints, no warnings and no progress on stderr. ncp still shows errors'
    complete -c ncp -n "__fish_ncp_using_subcommand secret; and __fish_seen_subcommand_from list" -l no-cache -d 'Do not read or write the cache: the protocol version and the tool list of each server'
    complete -c ncp -n "__fish_ncp_using_subcommand secret; and __fish_seen_subcommand_from list" -l refresh -d 'Do not read the cache, but write it: ncp finds the protocol version and gets the tool list again. --no-cache has priority over --refresh'
    complete -c ncp -n "__fish_ncp_using_subcommand secret; and __fish_seen_subcommand_from list" -s h -l help -d 'Print help (see more with \'--help\')'
    complete -c ncp -n "__fish_ncp_using_subcommand secret; and __fish_seen_subcommand_from list" -s V -l version -d 'Print version'
    complete -c ncp -n "__fish_ncp_using_subcommand secret; and __fish_seen_subcommand_from delete" -l server -d 'Delete only the item of the destination of this server' -r
    complete -c ncp -n "__fish_ncp_using_subcommand secret; and __fish_seen_subcommand_from delete" -l timeout -d 'The time limit in whole seconds for one request, a login or an input. After a timeout, ncp cancels the request or stops the connection' -r
    complete -c ncp -n "__fish_ncp_using_subcommand secret; and __fish_seen_subcommand_from delete" -l output-format -d 'The output format. Default: text' -r -f -a "text\t'Text for people'
    json\t'One JSON value on stdout, and JSON Lines on stderr'"
    complete -c ncp -n "__fish_ncp_using_subcommand secret; and __fish_seen_subcommand_from delete" -l config-file -d 'Use this configuration file in place of the global file: ncp/mcp.json in the XDG configuration directory, by default ~/.config/ncp/mcp.json. The file must exist. ncp then does not search for a project .mcp.json, and runs the servers of this file with no trust question' -r -F
    complete -c ncp -n "__fish_ncp_using_subcommand secret; and __fish_seen_subcommand_from delete" -l unused -d 'Delete the items that no server of the global file or of the project file of the current directory uses. A project in another directory can still use them'
    complete -c ncp -n "__fish_ncp_using_subcommand secret; and __fish_seen_subcommand_from delete" -l all -d 'Delete the items of all destinations'
    complete -c ncp -n "__fish_ncp_using_subcommand secret; and __fish_seen_subcommand_from delete" -s v -l verbose -d 'Show log lines on stderr: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change'
    complete -c ncp -n "__fish_ncp_using_subcommand secret; and __fish_seen_subcommand_from delete" -l timings -d 'Show the time of each step on stderr: CPU, configuration, credentials, server start, DNS, connect and each response'
    complete -c ncp -n "__fish_ncp_using_subcommand secret; and __fish_seen_subcommand_from delete" -s q -l quiet -d 'Show no hints, no warnings and no progress on stderr. ncp still shows errors'
    complete -c ncp -n "__fish_ncp_using_subcommand secret; and __fish_seen_subcommand_from delete" -l no-cache -d 'Do not read or write the cache: the protocol version and the tool list of each server'
    complete -c ncp -n "__fish_ncp_using_subcommand secret; and __fish_seen_subcommand_from delete" -l refresh -d 'Do not read the cache, but write it: ncp finds the protocol version and gets the tool list again. --no-cache has priority over --refresh'
    complete -c ncp -n "__fish_ncp_using_subcommand secret; and __fish_seen_subcommand_from delete" -s h -l help -d 'Print help (see more with \'--help\')'
    complete -c ncp -n "__fish_ncp_using_subcommand secret; and __fish_seen_subcommand_from delete" -s V -l version -d 'Print version'
    complete -c ncp -n "__fish_ncp_using_subcommand trust" -l timeout -d 'The time limit in whole seconds for one request, a login or an input. After a timeout, ncp cancels the request or stops the connection' -r
    complete -c ncp -n "__fish_ncp_using_subcommand trust" -l output-format -d 'The output format. Default: text' -r -f -a "text\t'Text for people'
    json\t'One JSON value on stdout, and JSON Lines on stderr'"
    complete -c ncp -n "__fish_ncp_using_subcommand trust" -l config-file -d 'Use this configuration file in place of the global file: ncp/mcp.json in the XDG configuration directory, by default ~/.config/ncp/mcp.json. The file must exist. ncp then does not search for a project .mcp.json, and runs the servers of this file with no trust question' -r -F
    complete -c ncp -n "__fish_ncp_using_subcommand trust" -l show -d 'Show the entries and the trust state, and change nothing'
    complete -c ncp -n "__fish_ncp_using_subcommand trust" -l untrust -d 'Remove the trust of the project file'
    complete -c ncp -n "__fish_ncp_using_subcommand trust" -l ignore -d 'Ignore the project file: ncp does not ask again until you run ncp trust'
    complete -c ncp -n "__fish_ncp_using_subcommand trust" -s v -l verbose -d 'Show log lines on stderr: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change'
    complete -c ncp -n "__fish_ncp_using_subcommand trust" -l timings -d 'Show the time of each step on stderr: CPU, configuration, credentials, server start, DNS, connect and each response'
    complete -c ncp -n "__fish_ncp_using_subcommand trust" -s q -l quiet -d 'Show no hints, no warnings and no progress on stderr. ncp still shows errors'
    complete -c ncp -n "__fish_ncp_using_subcommand trust" -l no-cache -d 'Do not read or write the cache: the protocol version and the tool list of each server'
    complete -c ncp -n "__fish_ncp_using_subcommand trust" -l refresh -d 'Do not read the cache, but write it: ncp finds the protocol version and gets the tool list again. --no-cache has priority over --refresh'
    complete -c ncp -n "__fish_ncp_using_subcommand trust" -s h -l help -d 'Print help (see more with \'--help\')'
    complete -c ncp -n "__fish_ncp_using_subcommand trust" -s V -l version -d 'Print version'
    complete -c ncp -n "__fish_ncp_using_subcommand generate-shell-completion" -l timeout -d 'The time limit in whole seconds for one request, a login or an input. After a timeout, ncp cancels the request or stops the connection' -r
    complete -c ncp -n "__fish_ncp_using_subcommand generate-shell-completion" -l output-format -d 'The output format. Default: text' -r -f -a "text\t'Text for people'
    json\t'One JSON value on stdout, and JSON Lines on stderr'"
    complete -c ncp -n "__fish_ncp_using_subcommand generate-shell-completion" -l config-file -d 'Use this configuration file in place of the global file: ncp/mcp.json in the XDG configuration directory, by default ~/.config/ncp/mcp.json. The file must exist. ncp then does not search for a project .mcp.json, and runs the servers of this file with no trust question' -r -F
    complete -c ncp -n "__fish_ncp_using_subcommand generate-shell-completion" -s v -l verbose -d 'Show log lines on stderr: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change'
    complete -c ncp -n "__fish_ncp_using_subcommand generate-shell-completion" -l timings -d 'Show the time of each step on stderr: CPU, configuration, credentials, server start, DNS, connect and each response'
    complete -c ncp -n "__fish_ncp_using_subcommand generate-shell-completion" -s q -l quiet -d 'Show no hints, no warnings and no progress on stderr. ncp still shows errors'
    complete -c ncp -n "__fish_ncp_using_subcommand generate-shell-completion" -l no-cache -d 'Do not read or write the cache: the protocol version and the tool list of each server'
    complete -c ncp -n "__fish_ncp_using_subcommand generate-shell-completion" -l refresh -d 'Do not read the cache, but write it: ncp finds the protocol version and gets the tool list again. --no-cache has priority over --refresh'
    complete -c ncp -n "__fish_ncp_using_subcommand generate-shell-completion" -s h -l help -d 'Print help (see more with \'--help\')'
    complete -c ncp -n "__fish_ncp_using_subcommand generate-shell-completion" -s V -l version -d 'Print version'
    complete -c ncp -n "__fish_ncp_using_subcommand server; and __fish_seen_subcommand_from remove; and test (count (commandline -opc)) -eq 3" -f -a "(ncp __complete-servers 2>/dev/null)"
    complete -c ncp -n "__fish_ncp_using_subcommand server; and __fish_seen_subcommand_from show; and test (count (commandline -opc)) -eq 3" -f -a "(ncp __complete-servers 2>/dev/null)"
    complete -c ncp -n "__fish_ncp_using_subcommand tool; and __fish_seen_subcommand_from list; and test (count (commandline -opc)) -eq 3" -f -a "(ncp __complete-servers 2>/dev/null)"
    complete -c ncp -n "__fish_ncp_using_subcommand tool; and __fish_seen_subcommand_from show; and test (count (commandline -opc)) -eq 3" -f -a "(ncp __complete-servers 2>/dev/null)"
    complete -c ncp -n "__fish_ncp_using_subcommand tool; and __fish_seen_subcommand_from call; and test (count (commandline -opc)) -eq 3" -f -a "(ncp __complete-servers 2>/dev/null)"
    complete -c ncp -n "__fish_ncp_using_subcommand resource; and __fish_seen_subcommand_from list; and test (count (commandline -opc)) -eq 3" -f -a "(ncp __complete-servers 2>/dev/null)"
    complete -c ncp -n "__fish_ncp_using_subcommand resource; and __fish_seen_subcommand_from read; and test (count (commandline -opc)) -eq 3" -f -a "(ncp __complete-servers 2>/dev/null)"
    complete -c ncp -n "__fish_ncp_using_subcommand prompt; and __fish_seen_subcommand_from list; and test (count (commandline -opc)) -eq 3" -f -a "(ncp __complete-servers 2>/dev/null)"
    complete -c ncp -n "__fish_ncp_using_subcommand prompt; and __fish_seen_subcommand_from get; and test (count (commandline -opc)) -eq 3" -f -a "(ncp __complete-servers 2>/dev/null)"
    complete -c ncp -n "__fish_ncp_using_subcommand auth; and __fish_seen_subcommand_from login; and test (count (commandline -opc)) -eq 3" -f -a "(ncp __complete-servers 2>/dev/null)"
    complete -c ncp -n "__fish_ncp_using_subcommand auth; and __fish_seen_subcommand_from logout; and test (count (commandline -opc)) -eq 3" -f -a "(ncp __complete-servers 2>/dev/null)"
    complete -c ncp -n "__fish_ncp_using_subcommand auth; and __fish_seen_subcommand_from status; and test (count (commandline -opc)) -eq 3" -f -a "(ncp __complete-servers 2>/dev/null)"
    complete -c ncp -n "__fish_ncp_using_subcommand tool; and __fish_seen_subcommand_from show; and test (count (string match -v -- '-*' (commandline -opc))) -eq 4" -f -a "(ncp __complete-tools (string match -v -- '-*' (commandline -opc))[4] 2>/dev/null)"
    complete -c ncp -n "__fish_ncp_using_subcommand tool; and __fish_seen_subcommand_from call; and test (count (string match -v -- '-*' (commandline -opc))) -eq 4" -f -a "(ncp __complete-tools (string match -v -- '-*' (commandline -opc))[4] 2>/dev/null)"
    "#);
}

#[test]
fn completion_script_for_an_unsupported_shell_is_a_usage_error() {
    let home = Home::new("completion-snapshot-elvish");
    ncp_snapshot!(home.filters(), home.ncp(["generate-shell-completion", "elvish"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: invalid value 'elvish' for '<SHELL>'
      [possible values: bash, zsh, fish]

      tip: a similar value exists: 'fish'

    For more information, try '--help'.
    ");
    let run = ncp_snapshot!(
        home.filters(),
        home.ncp(["--output-format", "json", "generate-shell-completion", "elvish"]),
        @r#"
    exit_code: 2 (failure)
    ----- stderr -----
    {"error":{"causes":["invalid value 'elvish' for '<SHELL>'\n  [possible values: bash, zsh, fish]\nFor more information, try '--help'."],"code":"usage","data":[],"exit":2,"hints":[{"command":"ncp --help","message":"To see the commands, run `ncp --help`"}],"message":"The command line is not valid"}}
    "#
    );
    run.error();
}

#[test]
fn hidden_server_completion_lists_names_and_starts_no_server() {
    let home = Home::new("completion-snapshot-servers");
    let mark = home.root.join("started");
    home.configure(&json!({
        "beta": {"command": "/bin/sh", "args": ["-c", "touch \"$HOME/started\""]},
        "alpha": {"command": "/bin/sh", "args": ["-c", "touch \"$HOME/started\""]}
    }));
    ncp_snapshot!(home.filters(), home.ncp(["__complete-servers"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    alpha
    beta
    ");
    assert!(!mark.exists(), "completion started a server");
}

#[test]
fn hidden_server_completion_is_silent_with_a_broken_configuration() {
    let home = Home::new("completion-snapshot-servers-broken");
    let path = home.config_path();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "{ not json").unwrap();
    let run =
        ncp_snapshot!(home.filters(), home.ncp(["__complete-servers"]), @"exit_code: 0 (success)");
    assert_eq!(run.0.stdout, [] as [u8; 0]);
    assert_eq!(run.0.stderr, [] as [u8; 0]);
}

#[test]
fn hidden_tool_completion_lists_the_cached_tool_names() {
    let home = Home::new("completion-snapshot-tools");
    let pid = home.root.join("server.pid");
    home.configure(
        &json!({"fx": {"command": FIXTURE, "args": ["cached-public"],
        "env": {"NCP_FIXTURE_PID_FILE": pid.display().to_string()}}}),
    );
    assert_eq!(home.run(&["tool", "list", "fx"]).code(), 0);
    std::fs::remove_file(&pid).unwrap();
    ncp_snapshot!(home.filters(), home.ncp(["__complete-tools", "fx"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    ask
    echo
    elicit
    elicit-choice
    elicit-legacy
    elicit-url
    env
    env-names
    fail
    header
    needs-page
    progress
    summary
    wait
    ");
    assert!(!pid.exists(), "completion started a server");
}

#[test]
fn hidden_tool_completion_is_empty_for_an_unknown_server() {
    let home = Home::new("completion-snapshot-tools-unknown");
    let run = ncp_snapshot!(home.filters(), home.ncp(["__complete-tools", "nope"]), @"exit_code: 0 (success)");
    assert_eq!(run.0.stdout, [] as [u8; 0]);
}

#[test]
fn root_help_lists_the_commands() {
    let home = Home::new("completion-snapshot-help");
    ncp_snapshot!(home.filters(), home.ncp(["--help"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    A command-line MCP client

    Usage: ncp [OPTIONS] <COMMAND>

    Commands:
      server                     Configured servers
      tool                       Tools of a server
      resource                   Resources of a server
      prompt                     Prompts of a server
      auth                       OAuth login of HTTP servers
      secret                     Secrets for ${NAME} in the configuration
      trust                      Trust the project .mcp.json of the current directory
      generate-shell-completion  Show the completion script for a shell

    Options:
          --timeout <SECONDS>
              The time limit in whole seconds for one request, a login or an input. After a timeout, ncp cancels the request or stops the connection
              
              [default: 300]

      -v, --verbose...
              Show log lines on stderr: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change

          --timings
              Show the time of each step on stderr: CPU, configuration, credentials, server start, DNS, connect and each response

      -q, --quiet...
              Show no hints, no warnings and no progress on stderr. ncp still shows errors

          --no-cache
              Do not read or write the cache: the protocol version and the tool list of each server

          --refresh
              Do not read the cache, but write it: ncp finds the protocol version and gets the tool list again. --no-cache has priority over --refresh

          --output-format <FORMAT>
              The output format. Default: text

              Possible values:
              - text: Text for people
              - json: One JSON value on stdout, and JSON Lines on stderr

          --config-file <PATH>
              Use this configuration file in place of the global file: ncp/mcp.json in the XDG configuration directory, by default ~/.config/ncp/mcp.json. The file must exist. ncp then does not search for a project .mcp.json, and runs the servers of this file with no trust question

      -h, --help
              Print help (see a summary with '-h')

      -V, --version
              Print version

    The usual steps:
      ncp server add <SERVER> <URL>
      ncp server list
      ncp server show <SERVER>
      ncp tool list <SERVER>
      ncp tool show <SERVER> <TOOL>
      ncp tool call <SERVER> <TOOL> [key=value...]

    A ${NAME} in an env, headers or oauth.clientSecret value comes from the
    environment, the native store or the credential file. To store a value,
    run:
      ncp secret set <NAME>

    Output is text. To get JSON, use --output-format json.
    ");
    ncp_snapshot!(home.filters(), home.ncp(["--output-format", "json", "--help"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    A command-line MCP client

    Usage: ncp [OPTIONS] <COMMAND>

    Commands:
      server                     Configured servers
      tool                       Tools of a server
      resource                   Resources of a server
      prompt                     Prompts of a server
      auth                       OAuth login of HTTP servers
      secret                     Secrets for ${NAME} in the configuration
      trust                      Trust the project .mcp.json of the current directory
      generate-shell-completion  Show the completion script for a shell

    Options:
          --timeout <SECONDS>
              The time limit in whole seconds for one request, a login or an input. After a timeout, ncp cancels the request or stops the connection
              
              [default: 300]

      -v, --verbose...
              Show log lines on stderr: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change

          --timings
              Show the time of each step on stderr: CPU, configuration, credentials, server start, DNS, connect and each response

      -q, --quiet...
              Show no hints, no warnings and no progress on stderr. ncp still shows errors

          --no-cache
              Do not read or write the cache: the protocol version and the tool list of each server

          --refresh
              Do not read the cache, but write it: ncp finds the protocol version and gets the tool list again. --no-cache has priority over --refresh

          --output-format <FORMAT>
              The output format. Default: text

              Possible values:
              - text: Text for people
              - json: One JSON value on stdout, and JSON Lines on stderr

          --config-file <PATH>
              Use this configuration file in place of the global file: ncp/mcp.json in the XDG configuration directory, by default ~/.config/ncp/mcp.json. The file must exist. ncp then does not search for a project .mcp.json, and runs the servers of this file with no trust question

      -h, --help
              Print help (see a summary with '-h')

      -V, --version
              Print version

    The usual steps:
      ncp server add <SERVER> <URL>
      ncp server list
      ncp server show <SERVER>
      ncp tool list <SERVER>
      ncp tool show <SERVER> <TOOL>
      ncp tool call <SERVER> <TOOL> [key=value...]

    A ${NAME} in an env, headers or oauth.clientSecret value comes from the
    environment, the native store or the credential file. To store a value,
    run:
      ncp secret set <NAME>

    Output is text. To get JSON, use --output-format json.
    ");
}

#[test]
fn bare_ncp_with_no_command_is_a_usage_error() {
    let home = Home::new("completion-snapshot-bare");
    ncp_snapshot!(home.filters(), home.ncp([] as [&str; 0]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    A command-line MCP client

    Usage: ncp [OPTIONS] <COMMAND>

    Commands:
      server                     Configured servers
      tool                       Tools of a server
      resource                   Resources of a server
      prompt                     Prompts of a server
      auth                       OAuth login of HTTP servers
      secret                     Secrets for ${NAME} in the configuration
      trust                      Trust the project .mcp.json of the current directory
      generate-shell-completion  Show the completion script for a shell

    Options:
          --timeout <SECONDS>       The time limit in whole seconds for one request, a login or an input. After a timeout, ncp cancels the request or stops the connection [default: 300]
      -v, --verbose...              Show log lines on stderr: where each value comes from (never the value), the proxy route and the cached protocol version. -vv also shows the JSON-RPC messages and the HTTP requests. ncp hides the credentials of the connection, and shows all other text with no change
          --timings                 Show the time of each step on stderr: CPU, configuration, credentials, server start, DNS, connect and each response
      -q, --quiet...                Show no hints, no warnings and no progress on stderr. ncp still shows errors
          --no-cache                Do not read or write the cache: the protocol version and the tool list of each server
          --refresh                 Do not read the cache, but write it: ncp finds the protocol version and gets the tool list again. --no-cache has priority over --refresh
          --output-format <FORMAT>  The output format. Default: text [possible values: text, json]
          --config-file <PATH>      Use this configuration file in place of the global file: ncp/mcp.json in the XDG configuration directory, by default ~/.config/ncp/mcp.json. The file must exist. ncp then does not search for a project .mcp.json, and runs the servers of this file with no trust question
      -h, --help                    Print help (see more with '--help')
      -V, --version                 Print version

    The usual steps:
      ncp server add <SERVER> <URL>
      ncp server list
      ncp server show <SERVER>
      ncp tool list <SERVER>
      ncp tool show <SERVER> <TOOL>
      ncp tool call <SERVER> <TOOL> [key=value...]

    A ${NAME} in an env, headers or oauth.clientSecret value comes from the
    environment, the native store or the credential file. To store a value,
    run:
      ncp secret set <NAME>

    Output is text. To get JSON, use --output-format json.
    ");
    let run = ncp_snapshot!(home.filters(), home.ncp(["--output-format", "json"]), @r#"
    exit_code: 2 (failure)
    ----- stderr -----
    {"error":{"causes":[],"code":"usage","data":[],"exit":2,"hints":[{"command":"ncp --help","message":"To see the commands, run `ncp --help`"}],"message":"The command line has no command"}}
    "#);
    run.error();
}
