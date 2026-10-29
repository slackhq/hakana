final class Box<T> { public function __construct(public T $value) {} }
function box<T>(T $x): Box<T> { return new Box($x); }
function nested(shape(?'flag' => bool) $s): void {
 $v=box(Shapes::idx($s, 'flag', false));
 if ($v->value) { print('true'); }
}
function separate(shape(?'flag' => bool) $s): void {
 $b=Shapes::idx($s, 'flag', false);
 $v=box($b);
 if ($v->value) { print('true'); }
}
