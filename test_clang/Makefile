all: sample.cpp.o sample.h.pch

CC=../target/debug/octo_clang
#CC=clang

clean:
	rm -f *.h.pch
	rm -f *.h.i
	rm -f *.cpp.i
	rm -f *.cpp.o

#sample.h.i: sample.h
#	$(CC) -x c++-header -E sample.h -DTEST -o sample.h.i
#
#sample.h.pch: sample.h.i
#	$(CC) -x c++-header -c sample.h.i -DTEST -o sample.h.pch

sample.h.pch: sample.h
	$(CC) -x c++-header -c sample.h -DTEST -o sample.h.pch

#sample.cpp.i: sample.cpp sample.h.pch
#	$(CC) -x c++ -E -frewrite-includes -DTEST -include-pch sample.h.pch sample.cpp -o sample.cpp.i
#
#sample.cpp.o: sample.cpp.i
#	$(CC) -x c++ -c -DTEST -include-pch sample.h.pch sample.cpp.i -o sample.cpp.o

sample.cpp.o: sample.cpp sample.h.pch
	$(CC) -x c++ -c -frewrite-includes -DTEST -include-pch sample.h.pch sample.cpp -o sample.cpp.o
