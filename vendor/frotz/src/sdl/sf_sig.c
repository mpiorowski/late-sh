/*
 * sf_sig.c - SDL interface, signal handling
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

#include <stdio.h>
#include <signal.h>
#include <stdlib.h>
#include <SDL.h>

#include "sf_frotz.h"

#ifdef WIN32

static void resethandlers(void)
{
	signal(SIGINT, SIG_DFL);
	signal(SIGFPE, SIG_DFL);
	signal(SIGSEGV, SIG_DFL);
} /* resethandlers */


static const char *signame(int sig)
{
	switch (sig) {
	case SIGINT:
		return "[SIGINT]";
	case SIGFPE:
		return "[SIGFPE]";
	case SIGSEGV:
		return "[SIGSEGV]";
	default:
		return "";
	}
} /* signame */


static void myhandler(int s)
{
	resethandlers();
	os_fatal("Signal %d received %s", s, signame(s));
} /* myhandler */


void sf_installhandlers(void)
{
	signal(SIGINT, myhandler);
	signal(SIGFPE, myhandler);
	signal(SIGSEGV, myhandler);
} /* sf_installhandlers */

#else

#ifndef NO_EXECINFO_H
#include <execinfo.h>
#endif

/* get REG_EIP from ucontext.h */
#ifndef NO_UCONTEXT_H
#ifndef __USE_GNU
#define __USE_GNU
#include <ucontext.h>
#endif
#endif

/* REG_EIP does not exist on 64bit CPU */
#if defined(__amd64__) || defined (__x86_64__)
#define _PROG_COUNTER REG_RIP
#else
#define _PROG_COUNTER REG_EIP
#endif

static struct {
	int sig;
	char *name;
} NAMES[] = {
	{SIGSEGV, "SIGSEGV"},
	{SIGFPE, "SIGFPE"},
	{SIGILL, "SIGILL"},
	{0, NULL}
};

static char *getsigname(int s)
{
	int i = 0;
	while (NAMES[i].name) {
		if (NAMES[i].sig == s)
			return NAMES[i].name;
		i++;
	}
	return NULL;
} /* getsigname */

static void bt_sighandler(int sig, siginfo_t * info, void * UNUSED (secret))
{

	void *trace[16];
	char *nam;
	int i, trace_size = 0;

	if (sig == SIGINT) {
		fprintf(stderr, "Emergency Exit (Signal SIGITNT received)\n");
		SDL_Quit();
		os_quit(EXIT_FAILURE);
	}

	/* Do something useful with siginfo_t */

	printf("\nInterpreter bug!\nSignal %d ", sig);
	if ((nam = getsigname(sig)))
		printf("[%s] ", nam);

	if (sig == SIGSEGV)
		printf(" [faulty address is %p]", info->si_addr);

	printf("\n");

	trace_size = backtrace(trace, 16);
	/* overwrite sigaction with caller's address */
	/* trace[1] = (void *) uc->uc_mcontext.gregs[_PROG_COUNTER]; */

	/* skip first stack frame (points here) */
	printf("Backtrace:\n");

	for (i = 1; i < trace_size; i++) {
		printf("  ");
		fflush(stdout);
		backtrace_symbols_fd(trace + i, 1, fileno(stdout));
	}

	os_quit(EXIT_SUCCESS);
} /* bt_sighandler */


void sf_installhandlers(void)
{

	/* Install our signal handler */
	struct sigaction sa;

	sa.sa_sigaction = (void *)bt_sighandler;
	sigemptyset(&sa.sa_mask);
	sa.sa_flags = SA_RESTART | SA_SIGINFO;

	sigaction(SIGSEGV, &sa, NULL);
	sigaction(SIGUSR1, &sa, NULL);
	sigaction(SIGFPE, &sa, NULL);
	sigaction(SIGINT, &sa, NULL);
	sigaction(SIGILL, &sa, NULL);

} /* sf_installhandlers */

#endif
