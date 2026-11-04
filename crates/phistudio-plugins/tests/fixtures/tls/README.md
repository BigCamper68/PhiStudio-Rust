# Untrusted local TLS fixture

The certificate is self-signed for localhost/127.0.0.1. Its key is public test
data, used only by an ephemeral loopback test server. Never deploy this key or
add this certificate to a production trust store. Tests assert that the normal
host TLS verifier rejects it before sending HTTP headers or credentials.
