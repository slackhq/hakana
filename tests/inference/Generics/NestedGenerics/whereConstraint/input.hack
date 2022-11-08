class A<T> {
  public function __construct(public T $t) {}
  public function getNonnull<Tu>(): Tu where T = ?Tu {
    if ($this->t is null) {
      throw new \Exception('bad');
    }
    /* HAKANA_FIXME[InvalidReturnStatement] todo fix this */
    return $this->t;
  }
}

function bar(A<?string> $a): string {
  return $a->getNonnull();
}