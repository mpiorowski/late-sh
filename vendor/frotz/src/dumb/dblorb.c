/*
 * dblorb.c - Blorb routines
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
 * Foundation, Inc., 59 Temple Place - Suite 330, Boston, MA 02111-1307, USA
 * Or visit http://www.fsf.org/
 */


#include "dfrotz.h"
#include "dblorb.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include <unistd.h>
#include <math.h>

#ifndef NO_BLORB

extern f_setup_t f_setup;

FILE *blorb_fp;
bb_result_t blorb_res;
bb_map_t *blorb_map;

extern bool quiet_mode;

static int isblorb(FILE *);

#define UnsignedToFloat(u) (((double)((long)(u - 2147483647L - 1))) + 2147483648.0)


/*
 * dumb_blorb_init
 *
 * Check if we're opening a Blorb file directly.  If not, check
 * to see if there's a separate Blorb file that looks like it goes
 * along with this Zcode file.  If we have a Blorb file one way or the
 * other, make a Blorb map.  If we opened a Blorb file directly, that
 * means that our executable is in that file and therefore we will look
 * for a ZCOD chunk and record its location so os_load_story() can find it.
 * Make sure the Blorb file is opened and with the file pointer blorb_fp.
 */
bb_err_t dumb_blorb_init(char *filename)
{
	FILE *fp;
	char *p;
	char *mystring;
	int  len1;
	int  len2;

	bb_err_t blorb_err;

	blorb_map = NULL;

	if ((fp = fopen(filename, "rb")) == NULL)
		return bb_err_Read;

	/* Is this really a Blorb file?
	 * If not, maybe we're loading a naked zcode file
	 * and our resources are in a separate blorb file.
	 */
	if (isblorb(fp)) {			/* Now we know to look */
		f_setup.exec_in_blorb = 1;	/* for zcode in the blorb */
		blorb_fp = fp;
	} else { /* Not a Blorb.  This must be a ZCODE file. */
		fclose(fp);
		len1 = strlen(filename) + strlen(EXT_BLORB);
		len2 = strlen(filename) + strlen(EXT_BLORB3);
		/* Was a Blorb file name provided? */
		if (f_setup.blorb_file != NULL)
			mystring = strdup(f_setup.blorb_file);
		else {
			/* Determine default Blorb file name. */
			mystring = malloc(MAX(len1, len2) * sizeof(char) + 1);
			memcpy(mystring, filename, MAX(len1, len2) * sizeof(char));
			p = strrchr(mystring, '.');
			if (p != NULL) *p = '\0';
			strncat(mystring, EXT_BLORB, len1);
		}

		/* Check if foo.blb is there. */
		if ((fp = fopen(mystring, "rb")) == NULL) {
			p = strrchr(mystring, '.');
			if (p != NULL) *p = '\0';
			strncat(mystring, EXT_BLORB3, len2);
			if (!(fp = fopen(mystring, "rb")))
				return bb_err_NoBlorb;
		}
		if (!isblorb(fp)) {
			fclose(fp);
			return bb_err_NoBlorb;
		}
		/* At this point we know that we're using a naked zcode file */
		/* with resources in a separate Blorb file. */
		blorb_fp = fp;
		f_setup.use_blorb = 1;
		if (f_setup.blorb_file == NULL && !quiet_mode)
			printf("Found Blorb file named %s.\n", mystring);

		/* Save the blorb file here for later reference. */
		f_setup.blorb_file = strdup(mystring);
	}

	/* Create a Blorb map from this file.
	 * This will fail if the file is not a valid Blorb file.
	 * From this map, we can now pick out any resource we need.
	 */
	blorb_err = bb_create_map(blorb_fp, &blorb_map);
	if (blorb_err != bb_err_None)
		return bb_err_Format;

	/* Locate the EXEC chunk within the blorb file and record its
	* location so os_load_story() can find it.
	*/
	if (f_setup.exec_in_blorb) {
		blorb_err = bb_load_chunk_by_type(blorb_map, bb_method_FilePos,
			&blorb_res, bb_ID_ZCOD, 0);
		f_setup.exec_in_blorb = 1;
		f_setup.blorb_file = strdup(f_setup.story_file);
		if (!quiet_mode)
			printf("Found zcode chunk in Blorb file.\n");
	}

	return blorb_err;
} /* dumb_blorb_init */


/*
 * isblorb
 *
 * Returns 1 if this file is a Blorb file, 0 if not.
 *
 */
static int isblorb(FILE *fp)
{
	char mybuf[4];

	if (fp == NULL)
		return 0;

	fread(mybuf, 1, 4, fp);
	if (strncmp(mybuf, "FORM", 4))
		return 0;

	fseek(fp, 4, SEEK_CUR);
	fread(mybuf, 1, 4, fp);

	if (strncmp(mybuf, "IFRS", 4))
		return 0;

	return 1;
} /* isblorb */

#endif /* NO_BLORB */
