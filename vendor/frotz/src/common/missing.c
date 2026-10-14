/*
 * missing.c - Assorted standard functions that may be missing on older systems
 *	Written by David Griffith <dave@661.org>
 *
 * This file is part of Frotz.
 *
 * Frotz is free software; you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation; either version 2 of the License, or
 * (at your option) any later version.
 *
 * Frotz is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program; if not, write to the Free Software
 * Foundation, Inc., 51 Franklin Street, Fifth Floor, Boston, MA 02110-1301 USA
 */

#include "frotz.h"
#include <stdlib.h>

#ifdef NO_STRRCHR
char *my_strrchr(const char *s, int c)
{
	const char *save;

	if (c == 0) return (char *)s + strlen(s);
		save = 0;
	while (*s) {
		if (*s == c)
			save = s;
		s++;
	}
	return (char *)save;
} /* my_strrchr */
#endif  /* NO_STRRCHR */


#ifdef NO_BASENAME
char *my_basename (const char *filename)
{
	char *p = strrchr(filename, '/');
	return p ? p + 1 : (char *)filename;
}
#endif  /* NO_BASENAME */


#ifdef NO_MEMMOVE
void *my_memmove(void *dest, const void *src, size_t n)
{
	char *d =(char *)dest;
	char *s =(char *)src;

	if(s == d) return dest;

	if(s < d) {	/* copy from back */
		s=s+n-1;
		d=d+n-1;
		while(n--) *d-- = *s--;
	} else		/* copy from front */
		while(n--) *d++=*s++;

	return dest;
} /* my_memmove */
#endif /* NO_MEMMOVE */


#ifdef NO_STRDUP
char *my_strdup(const char *src)
{
	char *str;
	char *p;
	int len = 0;

	while (src[len])
		len++;
	str = malloc(len + 1);
	p = str;
	while (*src)
	*p++ = *src++;
	*p = '\0';
	return str;
} /* my_strdup */
#endif /* NO_STRDUP */


#ifdef NO_STRNDUP
char *my_strndup(const char *src, size_t n)
{
	char *str;
	char *p;
	int len = 0;

	while (src[len] && len < n)
		len++;
	str = malloc(len + 1);
	p = str;
	while (len--) {
		*p++ = *src++;
	}
	*p = '\0';
	return str;
} /* my_strndup */
#endif /* NO_STRNDUP */
