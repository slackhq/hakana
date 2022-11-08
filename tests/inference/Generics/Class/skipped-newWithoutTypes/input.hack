class MyCollection<Tv> {
  public function __construct(public vec<Tv> $members) {
    $this->members = $members;
  }
}

function getMixedCollection(string $s): MyCollection<mixed> {
  $collection = new MyCollection(vec[$s]);
  return $collection;
}