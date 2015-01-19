all: sample.cpp.o sample.obj

clean:
	del *.h.pch
	del *.cpp.o

sample.h.pch: sample.h
	octo_cl /c /Ycsample.h /Fpsample.h.pch /Foprecompiled.cpp.o precompiled.cpp

sample.cpp.o: sample.h.pch
	octo_cl /c /Yusample.h /Fpsample.h.pch /Fosample.cpp.o sample.cpp

sample.obj: sample.h.pch
	octo_cl /c /Yusample.h /Fpsample.h.pch sample.cpp

