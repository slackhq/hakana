strpos('foo', 'bar');
str_replace('foo', 'bar', 'baz');
substr('foo', 1);
count(vec[1, 2, 3]);
$c = newCounter();
$c->count();
(new Counter())->count();
Counter::count();

final class Counter {
	static function count(): void {}
}

function newCounter(): Counter {
	return new Counter();
}
