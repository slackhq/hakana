<<file: __EnableUnstableFeatures('case_types')>>

final class Column<T> {
	public function __construct(public string $name)[] {}
}

enum class SomeTableColumns: Column<arraykey> {
	Column<int> id = new Column('id');
	Column<int> latest_ts = new Column('latest_ts');
}

case type Operand<TKeys, T as ?arraykey> = T | \HH\EnumClass\Label<TKeys, Column<T>> | Sql<TKeys, T>;

final class Sql<TKeys, +TValue> {
	private function __construct() {}

	public static function greatest<TK, T as int>(Operand<TK, T> $a, Operand<TK, T> $b): Sql<TK, T> {
		invariant_violation('stub');
	}
}

final class SetExpression<TKeys> {
	private function __construct() {}

	public static function set<TK, T as ?arraykey>(
		\HH\EnumClass\Label<TK, Column<T>> $column,
		Operand<TK, T> $value,
	): SetExpression<TK> {
		return new SetExpression();
	}
}

final class SomeTableWriter {
	public function updateWithExpressions(vec<SetExpression<SomeTableColumns>> $expressions): void {}
}

function from_later_use(SomeTableWriter $writer, int $ts, bool $b): void {
	// Only updateWithExpressions binds TK, which hh_client infers from
	$expressions = vec[
		SetExpression::set(#id, #id),
		SetExpression::set(#latest_ts, Sql::greatest(#latest_ts, $ts)),
	];
	if ($b) {
		$expressions[] = SetExpression::set(#id, 5);
	}
	$writer->updateWithExpressions($expressions);
}
