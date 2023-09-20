class A {
  public function foo(): string {
    return "a";
  }
}

function a(A $a): void {
    if ($a->foo() is string) {}
}

function b(A $a): void {
    if (!($a->foo() is string)) {}
}