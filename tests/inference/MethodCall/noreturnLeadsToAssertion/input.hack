final class A {
  public function foo(): noreturn {
    exit();
  }
  
  public function bar(?string $s): string {
    if ($s is null) {
      $this->foo();
    }
    return $s;
  }
}