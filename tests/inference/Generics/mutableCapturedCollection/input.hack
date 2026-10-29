use namespace HH\Lib\C;
final class Ref<T> { public function __construct(public T $value) {} }
function inspect_empty(): void {
 $queries = new Ref(vec<string>[]);
 $add = () ==> { $queries->value[] = 'x'; };
 $read = () ==> { $q = C\lastx($queries->value); print($q); };
 $add();
 $read();
}
function reset(): void {
 $r = new Ref(vec<string>[]);
 $add = () ==> { $r->value[] = 'x'; };
 $reset = () ==> { $r->value = vec[]; };
 $add();
 $reset();
}
function infer_from_append(): void {
 $r = new Ref(vec[]);
 $add = () ==> { $r->value[] = 'x'; };
 $read = (): string ==> C\lastx($r->value);
 $add();
 print($read());
}
function dictionary(): void {
 $r = new Ref(dict<string, shape('value' => int)>[]);
 $add = (string $key) ==> { $r->value[$key] = shape('value' => 1); };
 $read = (): int ==> C\lastx($r->value)['value'];
 $add('x');
 print((string)$read());
}
function keyset_collection(): void {
 $r = new Ref(keyset<string>[]);
 $add = () ==> { $r->value[] = 'x'; };
 $read = (): keyset<string> ==> $r->value;
 $add();
 $read();
}
function named_collection(): void {
 $r = new Ref(Vector{1});
 $add = () ==> { $r->value[] = 2; };
 $read = (): Vector<int> ==> $r->value;
 $add();
 $read();
}
function mutable_boolean(): void {
 $r = new Ref(false);
 $check = () ==> { if ($r->value) { print('set'); } };
 $set = () ==> { $r->value = true; };
 $set();
 $check();
}
function mutable_shape(): void {
 $r = new Ref(shape('a' => 0, 'b' => 0));
 $set = () ==> { $r->value['a'] = 1; };
 $read = (): int ==> $r->value['a'] + $r->value['b'];
 $set();
 print((string)$read());
}
function increment(string $key): int {
 $r = new Ref(dict<string, int>[]);
 $r->value[$key] = ($r->value[$key] ?? 0) + 1;
 return HH\Lib\Math\sum($r->value);
}
function keep_tuple((int, string) $value): Ref<(int, string)> {
 return new Ref($value);
}
function unknown_write(): void {
 $r = new Ref(null);
 $set = ($value) ==> { $r->value = $value; };
 $set('value');
 if ($r->value is nonnull) {
  print('set');
 }
}
