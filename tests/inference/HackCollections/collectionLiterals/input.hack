final class A {
	public function getMap(): Map<string, int> {
		$m = Map {};
		$m['a'] = 1;
		return $m;
	}

	public function getVector(): Vector<string> {
		$v = Vector {'a'};
		$v[] = 'b';
		echo $v[0];
		return $v;
	}

	public function getSet(): Set<string> {
		$s = Set {};
		$s->add('b');
		return $s;
	}

	public function getPair(): Pair<int, string> {
		$p = Pair {1, 'a'};
		echo $p[1];
		return $p;
	}

	public function getMapValue(Map<string, int> $m): int {
		return $m['a'] ?? 0;
	}
}
