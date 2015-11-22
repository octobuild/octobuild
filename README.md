# Overview

[![Join the chat at https://gitter.im/bozaro/octobuild](https://badges.gitter.im/Join%20Chat.svg)](https://gitter.im/bozaro/octobuild?utm_source=badge&utm_medium=badge&utm_campaign=pr-badge&utm_content=badge) [![Build Status](https://travis-ci.org/bozaro/octobuild.svg?branch=master)](https://travis-ci.org/bozaro/octobuild) [![Build Status](https://builder.bozaro.ru/buildStatus/icon?job=octobuild-win/master)](https://builder.bozaro.ru/job/octobuild-win/branch/master/)

This project allows you to cache the compilation on Unreal Engine building (like ccache).

It's supported out of box (you need simply install it):

 * Visual Studio UBT build on Windows;
 * clang UBT build on Linux.

This program uses UBT extension point for IncrediBuild.

It speeds up recompilation by caching previous compilations and detecting when the same compilation is being done again.

## Known issues

On Windows you can't mix compilation with and without octobuild.