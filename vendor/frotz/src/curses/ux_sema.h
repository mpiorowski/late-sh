/*
 * ux_sema.h - semaphore abstraction for frotz
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
 *
 * This file and only this file is dual licensed under the MIT license.
 *
 * Copyright (c) 2019 Mark McCurry
 */

/*
 * For anything other than MAC OS X, this is a simple wrapper for unnamed
 * POSIX Semaphores.  For MAC OS X use the Grand Central Dispatch as suggested
 * at https://stackoverflow.com/questions/27736618/why-are-sem-init-sem-getvalue-sem-destroy-deprecated-on-mac-os-x-and-w 
 */

#ifndef CURSES_UX_SEMA_H
#define CURSES_UX_SEMA_H

#include "../common/defs.h"

#ifndef NO_SOUND

#ifdef MACOS
#include <dispatch/dispatch.h>
struct ux_sema {
	dispatch_semaphore_t dsem;
};
typedef struct ux_sema ux_sem_t;


/*
 * Note that the current implementation does not check return codes
 * so use void to make things simpler on the MAC side
 */
static inline void  ux_sem_init(ux_sem_t *sem, int pshared, unsigned int value)
{
	dispatch_semaphore_t *sema = &sem->dsem;
	(void) pshared;

	*sema = dispatch_semaphore_create(value);
}


static inline void  ux_sem_post(ux_sem_t *sem)
{
	if (sem->dsem != NULL)
		dispatch_semaphore_signal(sem->dsem);
}


static inline void ux_sem_wait(ux_sem_t *sem)
{
	if (sem->dsem != NULL)
		dispatch_semaphore_wait(sem->dsem, DISPATCH_TIME_FOREVER);
}


static inline int ux_sem_trywait(ux_sem_t *sem)
{
	if (sem->dsem != NULL)
		return (int) dispatch_semaphore_wait(sem->dsem, DISPATCH_TIME_NOW);
	return -1;
}
#else
#include <semaphore.h>
typedef sem_t ux_sem_t;


/*
 * Note that the current implementation does not check return codes
 * so use void to make things simpler on the MAC side
 */
static inline void  ux_sem_init(ux_sem_t *sem, int pshared, unsigned int value)
{
	(void) sem_init(sem, pshared, value);
}


static inline void  ux_sem_post(ux_sem_t *sem)
{
	(void) sem_post(sem);
}


static inline void ux_sem_wait(ux_sem_t *sem)
{
	(void) sem_wait(sem);
}


static inline int ux_sem_trywait(ux_sem_t *sem)
{
	return sem_trywait(sem);
}
#endif

#endif /* NO_SOUND */

#endif
