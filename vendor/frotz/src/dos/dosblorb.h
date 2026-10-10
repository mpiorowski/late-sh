/*
 * dosblorb.h
 *
 * Blorb related functions specific to the DOS interface.
 *
 */

#ifndef DOSBLORB_H
#define DOSBLORB_H

#ifndef NO_BLORB

#include "blorb.h"
#include "blorblow.h"


/*
 * The bb_result_t struct lacks a few members that would make things a
 * bit easier.  The myresource struct takes encapsulates the bb_result_t
 * struct and adds a type member and a filepointer.  I would like to
 * convince Andrew Plotkin to make a change in the reference Blorb code
 * to add these members.
 *
 */
typedef struct {
	bb_result_t bbres;
	unsigned long type;
	FILE *fp;
} myresource;

/* These are used only locally */
/*
extern bb_err_t		blorb_err;
extern bb_map_t		*blorb_map;
extern FILE		*blorb_fp;
*/
extern bb_result_t	blorb_res;

bb_err_t dos_blorb_init(char *);
void dos_blorb_stop(void);

#endif
#endif
