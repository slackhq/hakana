type t = shape('a' => int, 'b' => string);

function foo(dict<int, t> $arr): dict<int, t> {
  foreach ($arr as $k => $_) {
    $arr[$k]['b'] = 'hello';
  }
  return $arr;
}
