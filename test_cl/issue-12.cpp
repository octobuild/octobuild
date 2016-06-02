#pragma warning(default : 4628)
#define FOO ::foo

class foo {
};

template <typename T> class bar {
};

typedef bar<FOO> buzz;
