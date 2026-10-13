; owfix.asm - Fix for gremlins in the Open Watcom C runtime library.
;
; This file is part of Frotz.
;
; Frotz is free software; you can redistribute it and/or modify
; it under the terms of the GNU General Public License as published by
; the Free Software Foundation; either version 2 of the License, or
; (at your option) any later version.
;
; Frotz is distributed in the hope that it will be useful,
; but WITHOUT ANY WARRANTY; without even the implied warranty of
; MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
; GNU General Public License for more details.
;
; You should have received a copy of the GNU General Public License
; along with this program; if not, write to the Free Software
; Foundation, Inc., 51 Franklin Street, Fifth Floor, Boston, MA 02110-1301 USA
; Or visit http://www.fsf.org/

_TEXT	segment word public 'CODE'

	assume	cs:_TEXT

; Open Watcom clibl.lib's internal functions __PIA and __PIS for huge
; pointer arithmetic do not really normalize pointers completely.  This is
; an alternate implementation which does.
;
; (The OW source code also has implementations of functions __PCA and __PCS,
; which do a better job at normalizing pointers, but they follow a different
; interface, and in any case are not actually included in the C library.)
;
; Input: dx:ax = pointer; cx:bx = long addend or subtrahend.
; Output: dx:ax = normalized pointer.

	public	__PIA,__PIS
__PIS:
	neg	cx
	neg	bx
	sbb	cx,0
__PIA:
	add	ax,bx			; Quickly add the low part (bx) of
					; the addend; this frees up bx
	sbb	bl,bl			; Let the segment component (dx)
	and	bl,0x10			; absorb any carry over
	add	dh,bl
	mov	ch,cl			; Add the high part of the addend
	mov	cl,4			; (cl)
	shl	ch,cl
	add	dh,ch
	mov	bx,ax			; Combine the higher 12 bits of the
	shr	bx,cl			; updated offset (ax) into the
	add	dx,bx			; segment component (dx)
	and	ax,0x000f		; Zap these 12 bits from ax
	retf				; We are done

_TEXT	ends

	end
