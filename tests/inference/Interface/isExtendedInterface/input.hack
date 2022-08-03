interface A {}
class B implements A {}

function qux(A $a): void { }

qux(new B());