<<__ConsistentConstruct>>
abstract class BaseConfig {
	private function __construct() {}

	public static final function build(): this {
		return new static();
	}

	// `self::` is a forwarding call, so the `this` return type of build()
	// keeps its static-ness in the calling context
	public static final function buildByTeam(): this {
		return self::build();
	}
}

final class ChildConfig extends BaseConfig {}

function build_child(): ChildConfig {
	return ChildConfig::buildByTeam();
}
