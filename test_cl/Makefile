all: sample.cpp.o sample2.cpp.o

clean:
	del *.h.pch
	del *.cpp.o
	del *.i

sample.h.pch: sample.h
	octo_cl /c /DTEST /Ycsample.h /Fpsample.h.pch /Foprecompiled.cpp.o precompiled.cpp
	
sample.cpp.o: sample.h.pch
	octo_cl /c /DTEST /Yusample.h /Fpsample.h.pch /Fosample.cpp.o sample.cpp
	
sample2.cpp.o: sample.h.pch
	octo_cl /c /DTEST /Yusample.h /Fpsample.h.pch /FIsample.h /Fosample2.cpp.o sample2.cpp
	