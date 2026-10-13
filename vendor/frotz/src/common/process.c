/* process.c - Interpreter loop and program control
 *	Copyright (c) 1995-1997 Stefan Jokisch
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

#ifdef DJGPP
#include "djfrotz.h"
#endif

zword zargs[8];
int zargc;

static int finished = 0;

static void __extended__(void);
static void __illegal__(void);

void (*op0_opcodes[0x10])(void) = {
	z_rtrue,
	z_rfalse,
	z_print,
	z_print_ret,
	z_nop,
	z_save,
	z_restore,
	z_restart,
	z_ret_popped,
	z_catch,
	z_quit,
	z_new_line,
	z_show_status,
	z_verify,
	__extended__,
	z_piracy
};

void (*op1_opcodes[0x10])(void) = {
	z_jz,
	z_get_sibling,
	z_get_child,
	z_get_parent,
	z_get_prop_len,
	z_inc,
	z_dec,
	z_print_addr,
	z_call_s,
	z_remove_obj,
	z_print_obj,
	z_ret,
	z_jump,
	z_print_paddr,
	z_load,
	z_call_n
};

void (*var_opcodes[0x40])(void) = {
	__illegal__,
	z_je,
	z_jl,
	z_jg,
	z_dec_chk,
	z_inc_chk,
	z_jin,
	z_test,
	z_or,
	z_and,
	z_test_attr,
	z_set_attr,
	z_clear_attr,
	z_store,
	z_insert_obj,
	z_loadw,
	z_loadb,
	z_get_prop,
	z_get_prop_addr,
	z_get_next_prop,
	z_add,
	z_sub,
	z_mul,
	z_div,
	z_mod,
	z_call_s,
	z_call_n,
	z_set_colour,
	z_throw,
	__illegal__,
	__illegal__,
	__illegal__,
	z_call_s,
	z_storew,
	z_storeb,
	z_put_prop,
	z_read,
	z_print_char,
	z_print_num,
	z_random,
	z_push,
	z_pull,
	z_split_window,
	z_set_window,
	z_call_s,
	z_erase_window,
	z_erase_line,
	z_set_cursor,
	z_get_cursor,
	z_set_text_style,
	z_buffer_mode,
	z_output_stream,
	z_input_stream,
	z_sound_effect,
	z_read_char,
	z_scan_table,
	z_not,
	z_call_n,
	z_call_n,
	z_tokenise,
	z_encode_text,
	z_copy_table,
	z_print_table,
	z_check_arg_count
};

void (*ext_opcodes[0x1d])(void) = {
	z_save,
	z_restore,
	z_log_shift,
	z_art_shift,
	z_set_font,
	z_draw_picture,
	z_picture_data,
	z_erase_picture,
	z_set_margins,
	z_save_undo,
	z_restore_undo,
	z_print_unicode,
	z_check_unicode,
	z_set_true_colour,	/* spec 1.1 */
	__illegal__,
	__illegal__,
	z_move_window,
	z_window_size,
	z_window_style,
	z_get_wind_prop,
	z_scroll_window,
	z_pop_stack,
	z_read_mouse,
	z_mouse_window,
	z_push_stack,
	z_put_wind_prop,
	z_print_form,
	z_make_menu,
	z_picture_table
};


extern void script_open (bool);

/*
 * init_process
 *
 * Initialize process variables.
 *
 */
void init_process(void)
{
	finished = 0;
} /* init_process */


/*
 * load_operand
 *
 * Load an operand, either a variable or a constant.
 *
 */
static void load_operand(zbyte type)
{
	zword value;

	if (type & 2) {		/* variable */
		zbyte variable;

		CODE_BYTE(variable)
		if (variable == 0)
			value = *sp++;
		else if (variable < 16)
			value = *(fp - variable);
		else {
			zword addr = z_header.globals + 2 * (variable - 16);
			LOW_WORD(addr, value)
		}
	} else if (type & 1) {	/* small constant */
		zbyte bvalue;

		CODE_BYTE(bvalue)
		value = bvalue;
	} else
		CODE_WORD(value)	/* large constant */
	zargs[zargc++] = value;
} /* load_operand */


/*
 * load_all_operands
 *
 * Given the operand specifier byte, load all (up to four) operands
 * for a VAR or EXT opcode.
 *
 */
static void load_all_operands(zbyte specifier)
{
	int i;

	for (i = 6; i >= 0; i -= 2) {
		zbyte type = (specifier >> i) & 0x03;

		if (type == 3)
			break;
		load_operand(type);
	}
} /* load_all_operands */


/*
 * interpret
 *
 * Z-code interpreter main loop
 *
 */
void interpret(void)
{
	/* If we got a save file on the command line, use it now. */
	if (f_setup.restore_mode == 1) {
		z_restore();
		f_setup.restore_mode = 0;
	}

	/* If we're supposed to start a transcript from the start. */
	if (f_setup.script_now == 1)
		script_open(TRUE);

	do {
		zbyte opcode;

/* FIXME may be able to do this without demacroing */
#ifdef TOPS20
		long pc;
		pc = (long) (  ( (long) pcp - (long) zmp) & 0x7ffff );
		CODE_BYTE(opcode)
#else
		CODE_BYTE(opcode)
#endif
		zargc = 0;

		if (opcode < 0x80) {	/* 2OP opcodes */
			load_operand((zbyte) (opcode & 0x40) ? 2 : 1);
			load_operand((zbyte) (opcode & 0x20) ? 2 : 1);
			var_opcodes[opcode & 0x1f] ();
		} else if (opcode < 0xb0) {	/* 1OP opcodes */
			load_operand((zbyte) (opcode >> 4));
			op1_opcodes[opcode & 0x0f] ();
		} else if (opcode < 0xc0) {	/* 0OP opcodes */
			op0_opcodes[opcode - 0xb0] ();
		} else {	/* VAR opcodes */
			zbyte specifier1;
			zbyte specifier2;
			if (opcode == 0xec || opcode == 0xfa) {		/* opcodes 0xec */
				CODE_BYTE(specifier1)			/* and 0xfa are */
				    CODE_BYTE(specifier2)		/* call opcodes */
				    load_all_operands(specifier1);	/* with up to 8 */
				load_all_operands(specifier2);		/* arguments    */
			} else {
				CODE_BYTE(specifier1)
				    load_all_operands(specifier1);
			}
			var_opcodes[opcode - 0xc0] ();
		}

#if defined(DJGPP) && !defined(NO_SOUND)
		if (end_of_sound_flag)
			end_of_sound();
#endif

		os_tick();
	} while (finished == 0);

	finished--;
} /* interpret */


/*
 * call
 *
 * Call a subroutine. Save PC and FP then load new PC and initialise
 * new stack frame. Note that the caller may legally provide less or
 * more arguments than the function actually has. The call type "ct"
 * can be 0 (z_call_s), 1 (z_call_n) or 2 (direct call).
 *
 */
void call(zword routine, int argc, zword * args, int ct)
{
	long pc;
	zword value;
	zbyte count;
	int i;

	if (sp - stack < 4)
		runtime_error(ERR_STK_OVF);

	GET_PC(pc)
	* --sp = (zword) (pc >> 9);
	*--sp = (zword) (pc & 0x1ff);
	*--sp = (zword) (fp - stack - 1);
	*--sp = (zword) (argc | (ct << 12));

	fp = sp;
	frame_count++;

	/* Calculate byte address of routine */

	if (z_header.version <= V3)
		pc = (long)routine << 1;
	else if (z_header.version <= V5)
		pc = (long)routine << 2;
	else if (z_header.version <= V7)
		pc = ((long)routine << 2) + ((long)z_header.functions_offset << 3);
	else			/* z_header.version == V8 */
		pc = (long)routine << 3;

	if (pc >= story_size)
		runtime_error(ERR_ILL_CALL_ADDR);

	SET_PC(pc)
	/* Initialise local variables */
	CODE_BYTE(count)

	if (count > 15)
		runtime_error(ERR_CALL_NON_RTN);
	if (sp - stack < count)
		runtime_error(ERR_STK_OVF);

	fp[0] |= (zword) count << 8;	 /* Save local var count for Quetzal. */
	value = 0;
	for (i = 0; i < count; i++) {
		if (z_header.version <= V4)	  /* V1 to V4 games provide default */
			CODE_WORD(value)  /* values for all local variables */
			*--sp = (zword) ((argc-- > 0) ? args[i] : value);
	}

	/* Start main loop for direct calls */
	if (ct == 2)
		interpret();
} /* call */


/*
 * ret
 *
 * Return from the current subroutine and restore the previous stack
 * frame. The result may be stored (0), thrown away (1) or pushed on
 * the stack (2). In the latter case a direct call has been finished
 * and we must exit the interpreter loop.
 *
 */
void ret(zword value)
{
	long pc;
	int ct;

	if (sp > fp)
		runtime_error(ERR_STK_UNDF);

	sp = fp;

	ct = *sp++ >> 12;
	frame_count--;
	fp = stack + 1 + *sp++;
	pc = *sp++;
	pc = ((long)*sp++ << 9) | pc;

	SET_PC(pc)
	/* Handle resulting value */
	if (ct == 0)
		store(value);
	if (ct == 2)
		*--sp = value;

	/* Stop main loop for direct calls */
	if (ct == 2)
		finished++;
} /* ret */

/*
 * branch
 *
 * Take a jump after an instruction based on the flag, either true or
 * false. The branch can be short or long; it is encoded in one or two
 * bytes respectively. When bit 7 of the first byte is set, the jump
 * takes place if the flag is true; otherwise it is taken if the flag
 * is false. When bit 6 of the first byte is set, the branch is short;
 * otherwise it is long. The offset occupies the bottom 6 bits of the
 * first byte plus all the bits in the second byte for long branches.
 * Uniquely, an offset of 0 means return false, and an offset of 1 is
 * return true.
 *
 */
void branch(bool flag)
{
	long pc;
	zword offset;
	zbyte specifier;
	zbyte off1;
	zbyte off2;

#ifdef TOPS20
	short soffset;
#endif

	CODE_BYTE(specifier)
	off1 = specifier & 0x3f;

	if (!flag)
		specifier ^= 0x80;

	if (!(specifier & 0x40)) {	/* it's a long branch */
		if (off1 & 0x20)	/* propagate sign bit */
			off1 |= 0xc0;

		CODE_BYTE(off2)
		offset = (off1 << 8) | off2;
	} else
		offset = off1;	/* it's a short branch */

	if (specifier & 0x80) {
		if (offset > 1) {	/* normal branch */
/* FIXME Does this need to be demacroed? */
#ifdef TOPS20
			GET_PC(pc)
			soffset = s16(offset);
			pc += soffset - 2;
			SET_PC(pc)
#else
			GET_PC(pc)
			pc += (short)offset - 2;
			SET_PC(pc)
#endif


		} else
			ret(offset);	/* special case, return 0 or 1 */
	}
} /* branch */


/*
 * store
 *
 * Store an operand, either as a variable or pushed on the stack.
 *
 */
void store(zword value)
{
	zbyte variable;

#ifdef TOPS20
	value &= 0xffff;
#endif
	CODE_BYTE(variable)

	if (variable == 0)
		*--sp = value;
	else if (variable < 16)
		*(fp - variable) = value;
	else {
		zword addr = z_header.globals + 2 * (variable - 16);
		SET_WORD(addr, value)
	}
} /* store */


/*
 * direct_call
 *
 * Call the interpreter loop directly. This is necessary when
 *
 * - a sound effect has been finished
 * - a read instruction has timed out
 * - a newline countdown has hit zero
 *
 * The interpreter returns the result value on the stack.
 *
 */
int direct_call(zword addr)
{
	zword saved_zargs[8];
	int saved_zargc;
	int i;
#ifdef TOPS20
	short sv;
#endif

	/* Calls to address 0 return false */
	if (addr == 0)
		return 0;

	/* Save operands and operand count */
	for (i = 0; i < 8; i++)
		saved_zargs[i] = zargs[i];
	saved_zargc = zargc;

	/* Call routine directly */
	call(addr, 0, 0, 2);

	/* Restore operands and operand count */
	for (i = 0; i < 8; i++)
		zargs[i] = saved_zargs[i];
	zargc = saved_zargc;

	/* Resulting value lies on top of the stack */
#ifdef TOPS20
	sv = s16(*sp);
	sp += 1;
	return (((zword) sv) & 0xffff);
#else
	return (short)*sp++;
#endif
} /* direct_call */


/*
 * __extended__
 *
 * Load and execute an extended opcode.
 *
 */
static void __extended__(void)
{
	zbyte opcode;
	zbyte specifier;

	CODE_BYTE(opcode)
	CODE_BYTE(specifier)
	load_all_operands(specifier);

	/* extended opcodes from 0x1d on */
	if (opcode < 0x1d)
		ext_opcodes[opcode] ();	/* are reserved for future spec' */
} /* __extended__ */


/*
 * __illegal__
 *
 * Exit game because an unknown opcode has been hit.
 *
 */
static void __illegal__(void)
{
	runtime_error(ERR_ILL_OPCODE);
} /* __illegal__ */


/*
 * z_catch, store the current stack frame for later use with z_throw.
 *
 *	no zargs used
 *
 */
void z_catch(void)
{
#ifdef TOPS20
	store(frame_count & 0xffff);
#else
	store(frame_count);
#endif
} /* z_catch */


/*
 * z_throw, go back to the given stack frame and return the given value.
 *
 *	zargs[0] = value to return
 *	zargs[1] = stack frame
 *
 */
void z_throw(void)
{
#ifdef TOPS20
	if (((zargs[1]) & 0xffff) > STACK_SIZE)
#else
	if (zargs[1] > frame_count)
#endif
		runtime_error(ERR_BAD_FRAME);

	/* Unwind the stack a frame at a time. */
	for (; frame_count > zargs[1]; --frame_count)
		fp = stack + 1 + fp[1];

#ifdef TOPS20
	ret ((zargs[0]) & 0xffff);
#else
	ret(zargs[0]);
#endif
} /* z_throw */


/*
 * z_call_n, call a subroutine and discard its result.
 *
 * 	zargs[0] = packed address of subroutine
 *	zargs[1] = first argument (optional)
 *	...
 *	zargs[7] = seventh argument (optional)
 *
 */
void z_call_n(void)
{
#ifdef TOPS20
	if (((zargs[0]) & 0xffff) != 0)
		call (((zargs[0]) & 0xffff), zargc - 1, zargs + 1, 1);
#else
	if (zargs[0] != 0)
		call(zargs[0], zargc - 1, zargs + 1, 1);
#endif
} /* z_call_n */


/*
 * z_call_s, call a subroutine and store its result.
 *
 * 	zargs[0] = packed address of subroutine
 *	zargs[1] = first argument (optional)
 *	...
 *	zargs[7] = seventh argument (optional)
 *
 */
void z_call_s(void)
{
#ifdef TOPS20
	if (((zargs[0]) & 0xffff) != 0)
		call (((zargs[0]) & 0xffff), zargc - 1, zargs + 1, 0);
#else
	if (zargs[0] != 0)
		call(zargs[0], zargc - 1, zargs + 1, 0);
#endif
	else
		store(0);
} /* z_call_s */


/*
 * z_check_arg_count, branch if subroutine was called with >= n arg's.
 *
 * 	zargs[0] = number of arguments
 *
 */
void z_check_arg_count(void)
{
#ifdef TOPS20
	if (fp == stack + STACK_SIZE)
		branch (((zargs[0]) & 0xffff) == 0);
	else
		branch (((zargs[0]) & 0xffff) <= (*fp & 0xff));
#else
	if (fp == stack + STACK_SIZE)
		branch(zargs[0] == 0);
	else
		branch(zargs[0] <= (*fp & 0xff));
#endif
} /* z_check_arg_count */


/*
 * z_jump, jump unconditionally to the given address.
 *
 *	zargs[0] = PC relative address
 *
 */
void z_jump(void)
{
	long pc;
#ifdef TOPS20
	short soffset;
#endif

	GET_PC(pc)
#ifdef TOPS20
	soffset = s16(zargs[0]);
	pc = pc + soffset - 2;
#else
	pc += (short)zargs[0] - 2;
#endif
	if (pc >= story_size)
		runtime_error(ERR_ILL_JUMP_ADDR);

	SET_PC(pc)
} /* z_jump */


/*
 * z_nop, no operation.
 *
 *	no zargs used
 *
 */
void z_nop(void)
{
	/* Do nothing */
} /* z_nop */


/*
 * z_quit, stop game and exit interpreter.
 *
 *	no zargs used
 *
 */
void z_quit(void)
{
	finished = 9999;
} /* z_quit */


/*
 * z_ret, return from a subroutine with the given value.
 *
 *	zargs[0] = value to return
 *
 */
void z_ret(void)
{
#ifdef TOPS20
	ret ((zargs[0]) & 0xffff);
#else
	ret(zargs[0]);
#endif
} /* z_ret */


/*
 * z_ret_popped, return from a subroutine with a value popped off the stack.
 *
 *	no zargs used
 *
 */
void z_ret_popped(void)
{
	ret(*sp++);
} /* z_ret_popped */


/*
 * z_rfalse, return from a subroutine with false (0).
 *
 * 	no zargs used
 *
 */
void z_rfalse(void)
{
	ret(0);
} /* z_rfalse */


/*
 * z_rtrue, return from a subroutine with true (1).
 *
 * 	no zargs used
 *
 */
void z_rtrue(void)
{
	ret(1);
} /* z_rtrue */
