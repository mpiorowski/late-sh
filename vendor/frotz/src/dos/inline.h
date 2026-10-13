/*
 * inline.h
 *
 * Inline definitions for DOS for use with Watcom C.
 *
 */

#ifndef DOS_INLINE_H
#define DOS_INLINE_H

#ifdef __WATCOMC__

/*
 * Inline functions for calling BIOS video and date/time services, and DOS
 * mouse services.
 */
word bios_video_ah(byte ah);
#pragma aux bios_video_ah = "int 0x10" parm [ah] value [ax] modify [bx cx dx];
word bios_video_ah_al_bh_bl_cx(byte ah, byte al, byte bh, byte bl, word cx);
#pragma aux bios_video_ah_al_bh_bl_cx = "int 0x10" \
					parm [ah] [al] [bh] [bl] [cx] \
					value [ax] modify [bx cx dx];
word bios_video_ah_bh_dh_dl(byte ah, byte bh, byte dh, byte dl);
#pragma aux bios_video_ah_bh_dh_dl = "int 0x10" parm [ah] [bh] [dh] [dl] \
				     value [ax] modify [bx cx dx];
word bios_video_ah_bh_cx_dx(byte ah, byte bh, word cx, word dx);
#pragma aux bios_video_ah_bh_cx_dx = "int 0x10" parm [ah] [bh] [cx] [dx] \
				     value [ax] modify [bx cx dx];
word bios_video_ah_cx(byte ah, word cx);
#pragma aux bios_video_ah_cx = "int 0x10" parm [ah] [cx] value [ax] \
			       modify [bx cx dx];
word bios_video_ax(word ax);
#pragma aux bios_video_ax = "int 0x10" parm [ax] value [ax] modify [bx cx dx];
word bios_video_ax_bl(word ax, byte bl);
#pragma aux bios_video_ax_bl = "int 0x10" parm [ax] [bl] value [ax] \
			       modify [bx cx dx];
/*
 * This is used for int 0x10, ah = 6 or 7, which may destroy bp on some
 * buggy BIOSes (per Ralf Brown's Interrupt List).
 */
word bios_video_ax_bh_ch_cl_dh_dl(word ax, byte bh, byte ch, byte cl,
						    byte dh, byte dl);
#pragma aux bios_video_ax_bh_ch_cl_dh_dl = "push bp", \
					   "int 0x10", \
					   "pop bp" \
    parm [ax] [bh] [ch] [cl] [dh] [dl] value [ax] modify [bx cx dx];
word bios_video_ax_bx(word ax, word bx);
#pragma aux bios_video_ax_bx = "int 0x10" parm [ax] [bx] value [ax] \
			       modify [bx cx dx];
word bios_video_ax_bx_cx_esdx(word ax, word bx, word cx, void _far *esdx);
#pragma aux bios_video_ax_bx_cx_esdx = "int 0x10" parm [ax] [bx] [cx] [es dx] \
				       value [ax] modify [bx cx dx];
word bios_video_ax_bx_dh_ch_cl(word ax, word bx, byte dh, byte ch, byte cl);
#pragma aux bios_video_ax_bx_dh_ch_cl = "int 0x10" \
					parm [ax] [bx] [dh] [ch] [cl] \
					value [ax] modify [bx cx dx];
word bios_video_ax_esdx_bh(word ax, void _far *esdx, byte bh);
#pragma aux bios_video_ax_esdx_bh = "int 0x10" parm [ax] [es dx] [bh] \
				    value [ax] modify [bx cx dx];
long bios_time_ah(byte ah);
#pragma aux bios_time_ah = "int 0x1a" parm [ah] value [cx dx] modify [ax bx];
word dos_mouse_ax(word ax);
#pragma aux dos_mouse_ax = "int 0x33" parm [ax] value [ax] modify [bx cx dx];
__int64 dos_mouse_ax_bx(word ax, word bx);
#pragma aux dos_mouse_ax_bx = "int 0x33" parm [ax] [bx] value [ax bx cx dx];
/*
 * Used for latching operations in EGA or VGA screen output.
 */
void video_latch(volatile byte);
#pragma aux video_latch = "" parm [al] modify [];

#endif
#endif
