function foo(vec<(int, int)> $mapped): vec<(int, int)> {
	return Vec\concat(vec[], $mapped);
}