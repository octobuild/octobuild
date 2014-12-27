all: sample.cpp.o precompiled.cpp.i

clean:
	del *.h.pch
	del *.h.i
	del *.cpp.i
	del *.cpp.o

sample.h.pch: sample.h
	cl /c /Ycsample.h /Fpsample.h.pch /Foprecompiled.cpp.o precompiled.cpp

precompiled.cpp.i: precompiled.cpp
	cl /P /Fiprecompiled.cpp.i precompiled.cpp

sample.cpp.i: sample.cpp sample.h.pch
	cl /P /Yusample.h.pch /Fisample.cpp.i sample.cpp

sample.cpp.o: sample.cpp.i
	cl /c /Yusample.h /Fpsample.h.pch /Fosample.cpp.o sample.cpp
