abstract class ResultBase<+T, +TErr> {}

final class ResultOk<+T> extends ResultBase<T, nothing> {
	public function __construct(private T $t) {}
}

final class ResultError<+TErr> extends ResultBase<nothing, TErr> {
	public function __construct(private string $message) {}
}

<<file: __EnableUnstableFeatures('case_types')>>
<<Hakana\RequireMostSpecificReturnType>>
case type ResultWithError<+T, +TErr> as ResultBase<T, TErr> = ResultOk<T> | ResultError<TErr>;

type Result<+T> = ResultWithError<T, mixed>;
