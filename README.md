SubFinder  [![Build Status](https://travis-ci.org/Hyperchaotic/SubFinder.svg?branch=master)](https://travis-ci.org/Hyperchaotic/SubFinder)
=======

## Synopsis

A little utility for downloading subtitles from opensubtitles.org for a file or a directory. Uses the opensubtitles hashing algorithmn, so use it on the original files prior to any transcoding.

## Motivation

Mainly a simple project to get to know Rust threading, synchronization, file and error handling.

## Installation

cargo build --release

## Usage
```
Usage: SubFinder <dir/filename> <lang>. Defaulting to "SubFinder * eng".

Examples:
    subfinder * eng
    subfinder *.avi eng
    subfinder breakdance.avi

For opensubtitles.org user name and password, create a text file in $HOME\.subfinder\subfinder.conf containing:
    username = "youruname";
    password = "yourpassword";
  ```  

## Contributors

Not looking for developing further.

Thank you https://github.com/joeyfeldberg for the XML-RPC code.
