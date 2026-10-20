#!/usr/bin/env bash
#
# Creates a local code-signing identity, "Edgeway Local Signing", in your
# login keychain. Run it once on each Mac where you build Edgeway yourself.
#
# Why: macOS remembers the Accessibility permission by the app's signature.
# An ad hoc signature changes with every build, so after each rebuild macOS
# forgets the permission and Edgeway asks again. With a stable identity the
# permission survives rebuilds and updates. build-macos.sh picks this
# identity up automatically.
#
# The certificate is self-signed and only trusted on this Mac for code
# signing. It does not replace Apple notarization for public downloads.
# macOS asks for your password once, to trust the certificate.
#
# Remove it any time in Keychain Access (login › My Certificates).
#
# Copyright (c) 2026 Rafe Uddaraj. MIT License.

set -euo pipefail

NAME="Edgeway Local Signing"
KEYCHAIN="$HOME/Library/Keychains/login.keychain-db"

if security find-identity -v -p codesigning | grep -q "\"$NAME\""; then
  echo "\"$NAME\" already exists. Nothing to do."
  exit 0
fi

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

cat > "$work/cert.cnf" <<EOF
[ req ]
distinguished_name = dn
x509_extensions = ext
prompt = no
[ dn ]
CN = $NAME
[ ext ]
basicConstraints = critical, CA:false
keyUsage = critical, digitalSignature
extendedKeyUsage = critical, codeSigning
EOF

openssl req -x509 -newkey rsa:2048 -nodes -days 3650 \
  -keyout "$work/key.pem" -out "$work/cert.pem" -config "$work/cert.cnf" >/dev/null 2>&1
pass="$(openssl rand -hex 16)"
openssl pkcs12 -export -legacy -inkey "$work/key.pem" -in "$work/cert.pem" \
  -name "$NAME" -out "$work/identity.p12" -passout "pass:$pass" >/dev/null 2>&1 \
  || openssl pkcs12 -export -inkey "$work/key.pem" -in "$work/cert.pem" \
       -name "$NAME" -out "$work/identity.p12" -passout "pass:$pass"

security import "$work/identity.p12" -k "$KEYCHAIN" -P "$pass" -T /usr/bin/codesign >/dev/null
security add-trusted-cert -r trustRoot -p codeSign -k "$KEYCHAIN" "$work/cert.pem"

echo "Created \"$NAME\". Rebuild with scripts/build-macos.sh, then grant Accessibility one last time."
