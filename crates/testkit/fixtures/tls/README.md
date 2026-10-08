# Test certificates

Test only. These files are not secret, and no system trusts them.

- `ca.pem`: a test CA ("ncp test CA"). Its key was removed after use.
- `server.pem` and `server.key`: a server certificate from the test CA for
  `localhost`, `127.0.0.1` and `::1`.

The certificates are valid from 2019-06-01 to 2126-01-01. macOS rejects a
TLS server certificate that is issued after 2019-07-01 and is valid for
more than 825 days, also from a CA that the user adds. The early start date
lets the certificates stay valid with no renewal.

Made with OpenSSL 4, EC P-256 keys:

```
openssl req -x509 -new -newkey ec -pkeyopt ec_paramgen_curve:P-256 -nodes \
  -keyout ca.key -out ca.pem \
  -not_before 20190601000000Z -not_after 21260101000000Z \
  -subj "/CN=ncp test CA" \
  -addext "basicConstraints=critical,CA:TRUE" \
  -addext "keyUsage=critical,keyCertSign,cRLSign"
openssl req -new -newkey ec -pkeyopt ec_paramgen_curve:P-256 -nodes \
  -keyout server.key -out server.csr -subj "/CN=localhost"
openssl x509 -req -in server.csr -CA ca.pem -CAkey ca.key -out server.pem \
  -not_before 20190601000000Z -not_after 21260101000000Z -extfile ext.cnf
```

`ext.cnf`:

```
basicConstraints=CA:FALSE
keyUsage=critical,digitalSignature
extendedKeyUsage=serverAuth
subjectAltName=DNS:localhost,IP:127.0.0.1,IP:::1
```
