/*
 * sf_msg_en.c - SDL interface, English messages
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
 * Or visit http://www.fsf.org/
 */

#include "sf_frotz.h"

const char *sf_msgstring(int id)
{
	const char *p = "";
	switch (id) {
	case IDS_BLORB_GLULX:
		p = "unsupported Glulx code\n\nYou have attempted to load a Blorb file containing a Glulx game.\n" "For this game you need a Glulx interpreter instead.";
		break;
	case IDS_BLORB_NOEXEC:
		p = "no Z code\n\nYou have attempted to load a Blorb file that does not contain any\n" "recognized game data. It may be a Blorb file containing just graphics or\n" "sound data for a game, with the game in a separate file.\nCheck for a " "file with the same name but an extension of .z5, .z6 or .z8\nand try " "loading that into Frotz instead.";
		break;
	case IDS_MORE:
		p = "[More]";
		break;
	case IDS_HIT_KEY_EXIT:
		p = "[Hit any key to exit.]";
		break;
	case IDS_TITLE:
		p = "SDL Frotz";
		break;
	case IDS_FATAL:
		p = "Frotz Fatal Error";
		break;
	case IDS_FROTZ:
		p = "Frotz";
		break;
	case IDS_FAIL_DIRECTSOUND:
		p = "Failed to initialize DirectSound";
		break;
	case IDS_FAIL_MODPLUG:
		p = "Failed to initialize MODPlug";
		break;
	case IDS_ABOUT_INFO:
		p = "{\\rtf1\\ansi{\\b Windows Frotz 1.10, written by David Kinder.\\line Another fine product of the Frobozz Magic Z-code Interpreter Company.}{\\line\\super{ }\\par}Windows Frotz is released under the terms of the GNU General Public License. See the file COPYING that is included with this program for details.{\\line\\super{ }\\par}Windows Frotz copyright David Kinder 2002-2006.\\line Frotz copyright Stefan Jokisch 1995-1997.{\\line\\super{ }\\par}Frotz was written by Stefan Jokisch, with additions by Jim Dunleavy and David Griffith. Windows Frotz uses jpeglib by the Independent JPEG Group; libpng by Guy Eric Schalnat, Andreas Dilger, Glenn Randers-Pehrson, and others; zlib by Jean-loup Gailly and Mark Adler; ModPlug by Olivier Lapicque; and libogg and libvorbis by the Xiph.org Foundation.}";
		break;
	case IDS_SAVE_TITLE:
		p = "Save the current game";
		break;
	case IDS_RESTORE_TITLE:
		p = "Restore a saved game";
		break;
	case IDS_LOAD_AUX_TITLE:
		p = "Load a portion of z-machine memory";
		break;
	case IDS_SAVE_AUX_TITLE:
		p = "Save a portion of z-machine memory";
		break;
	case IDS_AUX_FILTER:
		p = "*|Any file";
		break;
	case IDS_SAVE_FILTER:
		p = "*" EXT_SAVE "|Saved games";
		break;
	case IDS_RECORD_TITLE:
		p = "Record input to a file";
		break;
	case IDS_PLAYBACK_TITLE:
		p = "Play back recorded input";
		break;
	case IDS_RECORD_FILTER:
		p = "*" EXT_COMMAND "|Record Files";
		break;
	case IDS_SCRIPT_TITLE:
		p = "Write out a script";
		break;
	case IDS_SCRIPT_FILTER:
		p = "*" EXT_SCRIPT "|Transcript Log Files";
		break;
	default:
		break;
	}

	return p;
} /* sf_msgstring */
