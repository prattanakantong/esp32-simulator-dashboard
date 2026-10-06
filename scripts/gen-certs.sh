#!/usr/bin/env bash
#
# gen-certs.sh: private CA, broker certificate, device certificates, revocation.
#
# Usage:
#   scripts/gen-certs.sh                  create the CA, the broker cert and the default devices
#   scripts/gen-certs.sh init             create the CA, the broker cert and an empty CRL
#   scripts/gen-certs.sh issue NAME...    issue a client certificate for each NAME (CN = NAME)
#   scripts/gen-certs.sh revoke NAME      revoke NAME's certificate and regenerate the CRL
#   scripts/gen-certs.sh crl              regenerate the CRL (it expires after CRL_DAYS)
#   scripts/gen-certs.sh status           list issued certificates and whether they are revoked
#
# Environment variables (all optional):
#   CERT_DIR         output directory                (default: <repo>/certs)
#   DEFAULT_CLIENTS  names created by the default run (default: "sim-01 sim-02 ingestor")
#   BROKER_CN        broker certificate CN           (default: localhost)
#   BROKER_SAN       broker subjectAltName           (default: DNS:localhost,IP:127.0.0.1)
#   DAYS_CA / DAYS_LEAF / CRL_DAYS                   (default: 3650 / 365 / 30)
#
# Needs OpenSSL 1.1.1 or newer. On Windows, run it from Git Bash.
# Private keys stay in CERT_DIR. Never commit that directory.

set -euo pipefail

# Git Bash on Windows rewrites arguments that look like paths (such as "/CN=name"). Turn that off.
export MSYS_NO_PATHCONV=1

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CERT_DIR="${CERT_DIR:-$ROOT/certs}"
DEFAULT_CLIENTS="${DEFAULT_CLIENTS:-sim-01 sim-02 ingestor}"
BROKER_CN="${BROKER_CN:-localhost}"
export BROKER_SAN="${BROKER_SAN:-DNS:localhost,IP:127.0.0.1}"
DAYS_CA="${DAYS_CA:-3650}"
DAYS_LEAF="${DAYS_LEAF:-365}"
CRL_DAYS="${CRL_DAYS:-30}"
CNF="openssl-ca.cnf"

# Names that would collide with files in the certificate directory.
RESERVED_NAMES="ca broker crl serial index crlnumber newcerts openssl-ca"

die() { echo "error: $*" >&2; exit 1; }
say() { echo "  $*"; }

# Run a command quietly; show its output only if it fails.
quiet() {
  local out
  if ! out=$("$@" 2>&1); then
    echo "$out" >&2
    return 1
  fi
}

valid_name() {
  # Same pattern the backend accepts for device IDs, and safe inside a certificate subject.
  [[ "$1" =~ ^[A-Za-z0-9_-]{1,64}$ ]] || return 1
  local r
  for r in $RESERVED_NAMES; do [[ "$1" != "$r" ]] || return 1; done
}

write_config() {
  [[ -f "$CNF" ]] && return 0
  cat > "$CNF" <<'EOF'
[ req ]
distinguished_name = req_dn
prompt             = no
x509_extensions    = v3_ca

[ req_dn ]
CN = IoT-Dev-CA

[ v3_ca ]
basicConstraints     = critical, CA:TRUE
keyUsage             = critical, keyCertSign, cRLSign
subjectKeyIdentifier = hash

[ ca ]
default_ca = CA_default

[ CA_default ]
dir             = .
database        = index.txt
new_certs_dir   = newcerts
serial          = serial
crlnumber       = crlnumber
certificate     = ca.crt
private_key     = ca.key
default_md      = sha256
default_days    = 365
default_crl_days = 30
unique_subject  = no
policy          = policy_cn
copy_extensions = none

[ policy_cn ]
commonName = supplied

[ server_ext ]
basicConstraints       = CA:FALSE
keyUsage               = critical, digitalSignature, keyEncipherment
extendedKeyUsage       = serverAuth
subjectAltName         = $ENV::BROKER_SAN
subjectKeyIdentifier   = hash
authorityKeyIdentifier = keyid

[ client_ext ]
basicConstraints       = CA:FALSE
keyUsage               = critical, digitalSignature
extendedKeyUsage       = clientAuth
subjectKeyIdentifier   = hash
authorityKeyIdentifier = keyid
EOF
}

gen_crl() {
  quiet openssl ca -config "$CNF" -gencrl -crldays "$CRL_DAYS" -out crl.pem
}

# Issue a certificate signed by the CA. Records it in the CA database so it can be revoked later.
issue_cert() {
  local name="$1" kind="$2" cn="$3"
  quiet openssl genrsa -out "$name.key" 2048
  chmod 600 "$name.key" 2>/dev/null || true
  quiet openssl req -new -key "$name.key" -subj "/CN=$cn" -config "$CNF" -out "$name.csr"
  quiet openssl ca -batch -config "$CNF" -extensions "${kind}_ext" \
    -days "$DAYS_LEAF" -notext -in "$name.csr" -out "$name.crt"
  rm -f "$name.csr"
  quiet openssl verify -CAfile ca.crt "$name.crt"
}

cmd_init() {
  write_config
  if [[ -f ca.key && ! -f index.txt ]]; then
    die "$CERT_DIR already holds a CA that was not created by this script (no CA database), so its certificates cannot be revoked here. Use a new CERT_DIR, or delete this directory and run again."
  fi
  if [[ -f ca.key && ! -f ca.crt ]] || [[ ! -f ca.key && -f ca.crt ]]; then
    die "ca.key and ca.crt must exist together. Remove the leftover file or the whole directory."
  fi

  mkdir -p newcerts
  [[ -f index.txt ]] || : > index.txt
  [[ -f crlnumber ]] || echo 1000 > crlnumber
  [[ -f serial ]] || openssl rand -hex 8 > serial

  if [[ ! -f ca.key ]]; then
    quiet openssl genrsa -out ca.key 4096
    chmod 600 ca.key 2>/dev/null || true
    quiet openssl req -x509 -new -key ca.key -sha256 -days "$DAYS_CA" \
      -config "$CNF" -extensions v3_ca -out ca.crt
    say "created CA: ca.crt (valid $DAYS_CA days)"
  else
    say "CA already exists: ca.crt"
  fi

  if [[ ! -f broker.crt ]]; then
    issue_cert broker server "$BROKER_CN"
    say "issued broker certificate (CN=$BROKER_CN, SAN=$BROKER_SAN)"
  else
    say "broker certificate already exists"
  fi

  # Mosquitto fails to start if crlfile points to a missing file, so always keep one.
  if [[ ! -f crl.pem ]]; then
    gen_crl
    say "created CRL: crl.pem (valid $CRL_DAYS days)"
  fi
}

cmd_issue() {
  [[ $# -ge 1 ]] || die "usage: gen-certs.sh issue NAME..."
  [[ -f ca.key ]] || die "no CA in $CERT_DIR. Run: gen-certs.sh init"
  local name
  for name in "$@"; do
    valid_name "$name" || die "invalid name '$name' (use letters, digits, '-' or '_', up to 64 characters; reserved: $RESERVED_NAMES)"
    if [[ -f "$name.crt" || -f "$name.key" ]]; then
      say "$name: files already exist, skipped (to replace it, revoke it and delete $name.crt/$name.key first)"
      continue
    fi
    issue_cert "$name" client "$name"
    say "issued client certificate: $name (CN=$name, valid $DAYS_LEAF days)"
  done
}

cmd_revoke() {
  [[ $# -eq 1 ]] || die "usage: gen-certs.sh revoke NAME"
  local name="$1"
  valid_name "$name" || die "invalid name '$name'"
  [[ -f "$name.crt" ]] || die "$name.crt not found in $CERT_DIR"

  local out
  if out=$(openssl ca -config "$CNF" -revoke "$name.crt" 2>&1); then
    say "revoked $name"
  elif grep -qi "already revoked" <<<"$out"; then
    say "$name was already revoked"
  else
    echo "$out" >&2
    die "could not revoke $name (was it issued by this script?)"
  fi

  gen_crl
  say "regenerated crl.pem"

  # Capture the output first: piping straight into grep -q can fail under pipefail (SIGPIPE).
  local check
  check=$(openssl verify -crl_check -CAfile ca.crt -CRLfile crl.pem "$name.crt" 2>&1 || true)
  if grep -qi "revoked" <<<"$check"; then
    say "check passed: OpenSSL reports $name as revoked"
  else
    echo "warning: OpenSSL did not report $name as revoked:" >&2
    echo "$check" >&2
  fi
  echo
  echo "Restart the broker so it reloads crl.pem. Revocation stops new connections;"
  echo "a session that is already connected stays up until it disconnects."
}

cmd_crl() {
  [[ -f ca.key ]] || die "no CA in $CERT_DIR. Run: gen-certs.sh init"
  gen_crl
  say "regenerated crl.pem (valid $CRL_DAYS days). Restart the broker to reload it."
}

cmd_status() {
  [[ -s index.txt ]] || { echo "no certificates issued yet"; return 0; }
  awk -F'\t' '{
    state = ($1 == "V") ? "active " : "REVOKED"
    e = $2
    printf "%s  expires 20%s-%s-%s  serial %s  %s\n", state, substr(e,1,2), substr(e,3,2), substr(e,5,2), $4, $6
  }' index.txt
}

print_hints() {
  local win
  win="$(pwd -W 2>/dev/null || pwd)"   # Git Bash: Windows-style path that Mosquitto understands
  cat <<EOF

Done. Files are in: $win

Mosquitto settings (use absolute paths, forward slashes):
  cafile   $win/ca.crt
  certfile $win/broker.crt
  keyfile  $win/broker.key
  crlfile  $win/crl.pem

Each device uses its own NAME.crt and NAME.key together with ca.crt.
Do not commit this directory: it contains private keys.
EOF
}

main() {
  command -v openssl >/dev/null 2>&1 || die "openssl not found in PATH (on Windows, run this from Git Bash)"
  local action="${1:-all}"
  [[ $# -eq 0 ]] || shift

  mkdir -p "$CERT_DIR"
  cd "$CERT_DIR"

  case "$action" in
    all)
      echo "Setting up certificates in $CERT_DIR"
      cmd_init
      # shellcheck disable=SC2086
      cmd_issue $DEFAULT_CLIENTS
      print_hints
      ;;
    init)   echo "Setting up CA in $CERT_DIR"; cmd_init; print_hints ;;
    issue)  cmd_issue "$@" ;;
    revoke) cmd_revoke "$@" ;;
    crl)    cmd_crl ;;
    status) cmd_status ;;
    -h|--help|help) awk 'NR > 1 && /^#/ { sub(/^# ?/, ""); print; next } NR > 1 { exit }' "${BASH_SOURCE[0]}" ;;
    *) die "unknown command '$action' (try: all, init, issue, revoke, crl, status, help)" ;;
  esac
}

main "$@"
