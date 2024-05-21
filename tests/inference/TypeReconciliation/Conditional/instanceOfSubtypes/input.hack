abstract class A {}
final class B extends A {}

abstract class C {}
final class D extends C {}

function makeA(): A {
  return new B();
}

function makeC(): C {
  return new D();
}

$a = rand(0, 1) ? makeA() : makeC();

if ($a is B || $a is D) { }