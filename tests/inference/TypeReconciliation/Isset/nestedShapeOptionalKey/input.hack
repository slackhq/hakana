function foo(shape(?'a' => shape('b' => int)) $arr): void {
	echo $arr['a']['b'] ?? '';
}