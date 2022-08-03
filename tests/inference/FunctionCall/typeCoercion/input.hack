class A {}
class B extends A{}

function fooFoo(B $b): void {}
fooFoo(new A());