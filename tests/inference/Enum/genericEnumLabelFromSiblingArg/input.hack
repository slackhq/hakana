<<file: __EnableUnstableFeatures('case_types')>>

final class Column<T> {
	public function __construct(public string $name)[] {}
}

enum class SomeTableColumns: Column<arraykey> {
	Column<int> id = new Column('id');
	Column<int> latest_ts = new Column('latest_ts');
}

case type Operand<TKeys, T as ?arraykey> = T | \HH\EnumClass\Label<TKeys, Column<T>> | Sql<TKeys, T>;

final class Cond<TKeys> {}

final class Sql<TKeys, +TValue> {
	private function __construct() {}

	public static function if<TK, T as ?arraykey>(
		(function(Cond<TK>): Cond<TK>) $condition,
		Operand<TK, T> $then,
		Operand<TK, T> $else,
	): Sql<TK, T> {
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

function from_typed_value(SomeTableWriter $writer, Sql<SomeTableColumns, int> $sql): void {
	// $sql binds TK before the label resolves
	$expressions = vec[SetExpression::set(#id, $sql)];
	$writer->updateWithExpressions($expressions);
}

function from_typed_closure(SomeTableWriter $writer): void {
	// $cond binds Sql::if's TK, and its Sql<SomeTableColumns, int> binds set's
	$cond = (Cond<SomeTableColumns> $c) ==> $c;
	$expressions = vec[SetExpression::set(#id, Sql::if($cond, 1, #id))];
	$writer->updateWithExpressions($expressions);
}
