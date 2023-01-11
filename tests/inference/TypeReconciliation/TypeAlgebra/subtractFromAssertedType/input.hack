class A {}
class B {}

function foo(?A $foo, B $other): ?A {
  if (rand(0, 1)) {
    $foo = $other;
  }
  if ($foo is A || !($foo is B)) {
    return $foo;
  }

  return null;
}