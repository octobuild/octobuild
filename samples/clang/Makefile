all: sample.cpp.o sample.h.i

clean:
	rm -f *.h.pch
	rm -f *.h.i
	rm -f *.cpp.i
	rm -f *.cpp.o

sample.h.pch: sample.h
	clang -x c++-header sample.h -o sample.h.pch

sample.h.i: sample.h
	clang -x c++-header sample.h -E -frewrite-includes -o sample.h.i

sample.cpp.i: sample.cpp sample.h.pch
	clang -x c++ -E -frewrite-includes -include-pch sample.h.pch sample.cpp -o sample.cpp.i

sample.cpp.o: sample.cpp.i
	clang -x c++ -c -include-pch sample.h.pch sample.cpp.i -o sample.cpp.o
