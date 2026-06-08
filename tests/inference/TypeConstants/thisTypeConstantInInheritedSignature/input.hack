final class SuccessOrError<T> {
	public function __construct(private ?T $value) {}
}

abstract class ServerCommand {
	abstract const type TResponse;

	const type TExecuteResult = SuccessOrError<this::TResponse>;

	abstract public function executeAsync(): Awaitable<this::TExecuteResult>;

	final protected static function success(this::TResponse $r): this::TExecuteResult {
		return new SuccessOrError($r);
	}
}

final class InitializeResult {}

final class InitializeCommand extends ServerCommand {
	const type TResponse = InitializeResult;

	<<__Override>>
	public async function executeAsync(): Awaitable<this::TExecuteResult> {
		return static::success(new InitializeResult());
	}
}
