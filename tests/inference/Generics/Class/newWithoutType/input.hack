final class MyCollection<Tv> {
	public function __construct(public vec<Tv> $members) {
		$this->members = $members;
	}
}

function takesMyCollectionMixed(MyCollection<mixed> $c): void {}
function takesMyCollectionInt(MyCollection<int> $c): void {}

function getMixedCollection(string $s): MyCollection<mixed> {
	$collection = new MyCollection(vec[$s]);
	return $collection;
}

function doMixedCollection(string $s): void {
	$collection = new MyCollection(vec[$s]);
	takesMyCollectionMixed($collection);
}

function getIntCollection(string $s): MyCollection<int> {
	$collection = new MyCollection(vec[$s]);
	return $collection;
}

function doIntCollection(string $s): void {
	$collection = new MyCollection(vec[$s]);
	takesMyCollectionInt($collection);
}