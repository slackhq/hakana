class A {
  const string C = 'bar';
}

function foo(shape('bar' => int, 'baz' => string) $arr): int {
    return $arr[A::C];
}