class MyCollection<Tv> {
  public function __construct(public vec<Tv> $members) {
    $this->members = $members;
  }
}

function getMixedCollection(string $s): MyCollection<mixed> {
  $collection = new MyCollection<_>(vec[$s]);
  return $collection;
}

function getIntCollection(string $s): MyCollection<int> {
  $collection = new MyCollection<_>(vec[$s]);
  return $collection;
}