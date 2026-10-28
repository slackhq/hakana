<<file: __EnableUnstableFeatures('case_types')>>

final class Column<T> {
	public function __construct(public string $name)[] {}
}

enum class SomeTableColumns: Column<arraykey> {
	Column<int> id = new Column('id');
	Column<int> latest_ts = new Column('latest_ts');
	Column<string> name = new Column('name');
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

	public static function upsertWithExpressions(vec<SetExpression<SomeTableColumns>> $expressions): void {}

	public function updateOne(SetExpression<SomeTableColumns> $expression): void {}
}

function update_with_bare_labels(SomeTableWriter $writer, int $ts): void {
	// hh_client solves TK from the vec<SetExpression<SomeTableColumns>> param
	$writer->updateWithExpressions(vec[
		SetExpression::set(#latest_ts, Sql::greatest(#latest_ts, $ts)),
		SetExpression::set(#id, 5),
	]);

	SomeTableWriter::upsertWithExpressions(vec[
		SetExpression::set(#latest_ts, Sql::greatest(#latest_ts, $ts)),
	]);

	$writer->updateOne(SetExpression::set(#name, 'foo'));
}
