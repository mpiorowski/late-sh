#!/usr/bin/perl -w

# This script is intended to speed up the process of adding new command 
# line parameters to the "help" output of text-mode programs.  Most 
# people want to have the progression of parameters go down the left 
# column and then down the right.  Prior to using this script, put your 
# parameter descriptions in a text file, one parameter per line.  Figure 
# out where you want the column to end and put that number in $middle.  
# This will cause the output to skip over that many spaces and then the 
# second column will begin.
#
# When you're all set up, run this script and give the filename of your 
# parameter text file.  Your parameters, formatted into two columns with 
# a '\t' for the tab in the middle, will be printed to STDOUT.  Now you 
# can put that text into a #define or a really long printf().
#
# In short, if your parameters file looks like this:
#   A
#   B
#   C
#   D
# You'll get this:
#   A   \t   C\n\
#   B   \t   D\n\
#
# I've seen scripts that do this before and forgot where they were, so I 
# made this one from scratch.  Wherever you find it, please feel free to 
# incorporate it into whatever project you feel appropriate.  I release 
# all rights to it to the public domain.
#
# Written in June 2017 by David Griffith <dave@661.org>
#

use POSIX;

my @lines;
my $leftside;
my $middle = 34;	# put a tab in the 35th column
my $skip;

open(INFILE, "<$ARGV[0]");
chomp(@lines = <INFILE>);
close(INFILE);	

@lines = grep(/\S/, @lines);

if (scalar @lines % 2 == 1) { push @lines, ""; } 

$leftsize = ceil(scalar @lines / 2);
for (my $i = 0; $i < $leftsize; $i++) {
	$skip = $middle - (2 + length($lines[$i]));
	print "  $lines[$i]";
	if ($lines[$i + $leftsize] eq "") {
		print "\\n\n";
	} else {
		print " " x $skip . "\\t $lines[$i + $leftsize]\\n\\\n";
	}
}


