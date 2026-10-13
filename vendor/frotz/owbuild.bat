@ECHO OFF
REM This batch file is for compiling Frotz using Open Watcom C for DOS.
if ""=="%WATCOM%" echo Cannot build --- WATCOM environment variable undefined!
if ""=="%WATCOM%" goto x
REM FIXME: this assumes PATH is correct if INCLUDE is set...
if ""=="%INCLUDE%" echo setting PATH=%WATCOM%\binw;%PATH%
if ""=="%INCLUDE%" set PATH=%WATCOM%\binw;%PATH%
if ""=="%INCLUDE%" echo setting INCLUDE=%WATCOM%\h
if ""=="%INCLUDE%" set INCLUDE=%WATCOM%\h
REM We are all set.
wmake -f makefile.ow %1
:x