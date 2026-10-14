/*
 * getopt.c
 *
 * Replacement for a Unix style getopt function
 *
 * Quick, clean, and portable to funky systems that don't have getopt()
 * for whatever reason.
 *
 */

#include <stdio.h>
#include <string.h>

int zoptind = 1;
int zoptopt = 0;
char *zoptarg = NULL;

int zgetopt (int argc, char *argv[], const char *options)
{
	static int pos = 1;
	const char *p;

	if (zoptind >= argc || argv[zoptind][0] != '-' || argv[zoptind][1] == 0)
		return EOF;

	zoptopt = argv[zoptind][pos++];
	zoptarg = NULL;

	if (argv[zoptind][pos] == 0) {
		pos = 1;
		zoptind++;
	}

	p = strchr(options, zoptopt);

	if (zoptopt == ':' || p == NULL) {
		fputs("illegal option -- ", stdout);
		goto error;
	} else if (p[1] == ':') {
		if (zoptind >= argc) {
			fputs("option requires an argument -- ", stdout);
			goto error;
		} else {
			zoptarg = argv[zoptind];
			if (pos != 1)
				zoptarg += pos;
			pos = 1;
			zoptind++;
		}
	}
	return zoptopt;

error:
	fputc(zoptopt, stdout);
	fputc('\n', stdout);

	return '?';
} /* zgetopt */
