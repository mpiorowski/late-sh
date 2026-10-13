/*
 * missing.h
 *
 * Declarations and definitions for standard things that may be missing
 * on older systems.  Currently all of them are from <string.h>.
 *
 * Name:	Originated	Standards
 *
 * strrchr3)	SVr4, 4.3BSD	POSIX.1-2001, POSIX.1-2008, C99
 * memmove(3)	SVr4, 4.3BSD	POSIX.1-2001, POSIX.1-2008
 * strdup(3)	SVr4, 4.3BSD	POSIX.1-2001
 * strndup(3)	GNU libc 2.x	POSIX.1-2008
 * basename(3)  GNU libc 2.x	X/Open Portability Guide Issue 4, Version 2
 *
 */

#ifndef MISSING_H
#define MISSING_H

#ifdef NO_STRRCHR
char *my_strrchr(const char *, int);
#define strrchr my_strrchr
#endif

#ifdef NO_MEMMOVE
void *my_memmove(void *, const void *, size_t);
#define memmove my_memmove
#endif

#ifdef NO_STRDUP
char *my_strdup(const char *);
#define strdup my_strdup
#endif

#ifdef NO_STRNDUP
char *my_strndup(const char *, size_t);
#define strndup my_strndup
#endif

#ifdef NO_BASENAME
char *my_basename(const char *);
#define basename my_basename
#endif

#endif /* MISSING_H */
