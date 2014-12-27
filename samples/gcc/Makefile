all: sample.cpp.o sample.h.i

clean:
	rm -f *.h.gch
	rm -f *.h.i
	rm -f *.cpp.i
	rm -f *.cpp.o

sample.h.gch: sample.h
	g++ -x c++-header sample.h -o sample.h.gch

sample.h.i: sample.h
	g++ -x c++-header sample.h -E -o sample.h.i

sample.cpp.i: sample.cpp sample.h.gch
	g++ -x c++ -E sample.cpp -o sample.cpp.i

sample.cpp.o: sample.cpp.i
	g++ -x c++ -c sample.cpp.i -o sample.cpp.o
