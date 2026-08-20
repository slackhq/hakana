<<__Sealed(HasGet::class, NoGet::class)>>
abstract class Base {}

final class HasGet extends Base {
	public function get(): string {
		return "ok";
	}
}

final class NoGet extends Base {
	public function reason(): int {
		return 0;
	}
}

// The guard block does not return on all paths: its trailing switch has a
// `default` case that falls off the end without break or return, and there is
// no return after the switch. So $res must stay HasGet|NoGet after the block
// and $res->get() must not be treated as always-HasGet.
function foo(Base $res): string {
	if (!$res is HasGet) {
		switch ($res->reason()) {
			case 1:
				return "one";
			default:
				// no break, no return - control falls through the switch
		}
		// no return here either - falls through the guard block
	}
	return $res->get();
}
