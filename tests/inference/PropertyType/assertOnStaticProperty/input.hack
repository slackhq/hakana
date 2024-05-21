final class A {
  public static ?string $s = null;
}

function foo(): string {
  if (A::$s is null) {
    A::$s = 'a';
  }
  return A::$s;
}