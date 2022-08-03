<?hh // strict
class Inv<T> {
  public function __construct(public T $item) { }
}

function test_vec_append(vec<string> $v):Inv<vec<arraykey>> {
  $obj = new Inv<vec<arraykey>>($v);
  $r = $obj->item;
  $r[] = 5;
  $obj->item = $r;
  return $obj;
}