final class A {
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

function c(dict<string, mixed> $args): void {
	if (($args['d'] ?? null) is nonnull) {}
}