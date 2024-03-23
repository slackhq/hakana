abstract class C {
  public function __construct(public ?int $i) {}
}

abstract class A {
  abstract const type T as C;

  public function returnCField(mixed $m): int {
    return ($m as this::T)->i ?? 0;
  }
}