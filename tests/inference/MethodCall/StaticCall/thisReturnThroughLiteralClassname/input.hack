abstract class Node {
	public static function fromJSON(dict<string, mixed> $_json): this {
		throw new \Exception('abstract');
	}
}

final class Leaf extends Node {}
final class Branch extends Node {}

function node_from_json(string $kind): ?Node {
	$kind_to_class = dict[
		'leaf' => Leaf::class,
		'branch' => Branch::class,
	];
	$class = $kind_to_class[$kind] ?? null;
	if ($class is nonnull) {
		return $class::fromJSON(dict[]);
	}
	return null;
}
