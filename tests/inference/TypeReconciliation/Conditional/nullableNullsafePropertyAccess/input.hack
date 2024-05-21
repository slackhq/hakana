final class A {
  public string $b = '';
}

function foo(?A $a): void {
    $c = $a?->b;
    if ($c is null) {}
}