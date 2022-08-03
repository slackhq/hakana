interface I {}

class A implements I {}

function takesI(KeyedContainer<string, mixed> $i): void {}

function takesA(KeyedContainer<string, mixed> $a): void {
  takesI($a);
}

