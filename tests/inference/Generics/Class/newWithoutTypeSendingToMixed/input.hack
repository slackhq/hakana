class MyCollection<Tv> {
	public function __construct(public vec<Tv> $members) {
		$this->members = $members;
	}
}

function takesMyCollectionMixed(MyCollection<mixed> $c): void {}

function getStringCollection(string $s): MyCollection<string> {
	$collection = new MyCollection(vec[$s]);
	takesMyCollectionMixed($collection);
	return $collection;
}