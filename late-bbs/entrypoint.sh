#!/usr/bin/env bash
set -euo pipefail

SBBS_HOME="${SBBS_HOME:-/bbs/sbbs}"
LORD_HOME="${LORD_HOME:-/bbs/doors/lord}"
SBBSCTRL="${SBBSCTRL:-${SBBS_HOME}/ctrl}"
SBBSEXEC="${SBBSEXEC:-${SBBS_HOME}/exec}"

mkdir -p /bbs/import /bbs/doors /bbs/dosemu /bbs/backups

if [[ ! -d "${SBBS_HOME}/ctrl" ]]; then
  echo "Initializing Synchronet data directory at ${SBBS_HOME}"
  mkdir -p "${SBBS_HOME}"
  cp -a /opt/sbbs/. "${SBBS_HOME}/"
fi

if [[ ! -e /sbbs ]]; then
  ln -s "${SBBS_HOME}" /sbbs
fi

mkdir -p "${LORD_HOME}"

if [[ ! -e "${SBBS_HOME}/exec/lord-runner" ]]; then
  ln -s /usr/local/bin/lord-runner "${SBBS_HOME}/exec/lord-runner"
fi

if [[ -f "${SBBSCTRL}/main.ini" ]]; then
  sed -i \
    -e 's/^name=.*/name=late.sh BBS/' \
    -e 's/^qwk_id=.*/qwk_id=LATESH/' \
    -e 's/^location=.*/location=Local Docker/' \
    -e 's/^password=.*/password=latebbs/' \
    -e 's/^\tquestions=.*/\tquestions=0x50401/' \
    "${SBBSCTRL}/main.ini"
fi

if [[ -f "${SBBSCTRL}/modopts.ini" ]]; then
  sed -i \
    -e 's/^\temail_passwords = .*/\temail_passwords = false/' \
    -e 's/^\tconfirm_email_address = .*/\tconfirm_email_address = false/' \
    -e 's/^\tbackspace = .*/\tbackspace = false/' \
    -e 's/^\tmouse = .*/\tmouse = false/' \
    -e 's/^\texascii = .*/\texascii = false/' \
    -e 's/^\tsurvey = .*/\tsurvey = false/' \
    "${SBBSCTRL}/modopts.ini"

  if ! grep -q 'late.sh LORD auto-launch' "${SBBSCTRL}/modopts.ini"; then
    cat >> "${SBBSCTRL}/modopts.ini" <<'EOF'

; late.sh LORD auto-launch
[logon]
	email_validation = false
	fast_logon = true
	show_avatar = false
	show_logon_list = false
	set_avatar = false
	eval_last = bbs.exec_xtrn("LORD"); bbs.hangup();

[xtrn:LORD]
	clear_screen_on_exec = true
	pause_after_info = false
EOF
  fi
fi

if [[ -f "${SBBSCTRL}/xtrn.ini" ]] && ! grep -q '^\[prog:GAMES:LORD\]' "${SBBSCTRL}/xtrn.ini"; then
  cat >> "${SBBSCTRL}/xtrn.ini" <<'EOF'

[prog:GAMES:LORD]
	name=Legend of the Red Dragon
	ars=ANSI
	execution_ars=
	type=0
	settings=0x4402
	event=0
	cost=0
	cmd=/bbs/sbbs/exec/lord-runner
	clean_cmd=
	startup_dir=/bbs/doors/lord/
	textra=0
	max_time=0
	max_inactivity=0
EOF
fi

if [[ -f "${SBBSCTRL}/sbbs.ini" ]] && ! grep -q "late.sh LORD BBS container defaults" "${SBBSCTRL}/sbbs.ini"; then
  cat >> "${SBBSCTRL}/sbbs.ini" <<'EOF'

; late.sh LORD BBS container defaults
[UNIX]
User=sbbs
Group=sbbs
EOF
fi

chown -R sbbs:sbbs /bbs

if [[ ! -x "${SBBSEXEC}/sbbs" ]]; then
  echo "Synchronet executable missing: ${SBBSEXEC}/sbbs" >&2
  exit 1
fi

echo "Starting Synchronet with SBBSCTRL=${SBBSCTRL}"
exec env SBBSCTRL="${SBBSCTRL}" SBBSEXEC="${SBBSEXEC}" "${SBBSEXEC}/sbbs"
