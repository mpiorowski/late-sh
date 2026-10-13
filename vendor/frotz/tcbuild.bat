@ECHO OFF
REM This batch file is for compiling Frotz using Turbo C 3.00 for DOS.
REM It's just a bit of syntactic sugar so I don't have to always 
REM remember that I need to specify the other Makefile for DOS 
REM compilation.  DG
make -f makefile.tc %1
