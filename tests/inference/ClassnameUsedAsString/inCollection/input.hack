namespace Foo {
	final class A {}

    function namespaced_keyset(): keyset<string> {
        return keyset[A::class];
    }

    function namespaced_dict(): dict<string, int> {
        return dict[A::class => 1];
    }
}

namespace {
	abstract class B {}

	final class C extends B {}

	function in_string_keyset(): keyset<string> {
		return keyset[
			Foo\A::class,
			B::class,
		];
	}

	function in_classname_keyset(): keyset<classname<B>> {
		return keyset[
			B::class,
			C::class,
		];
	}

	function as_dict_key(): dict<string, int> {
		return dict[
			Foo\A::class => 1,
			B::class => 2,
		];
	}

	function as_dict_classname_key(): dict<classname<B>, int> {
		return dict[
			B::class => 1,
			C::class => 2,
		];
	}

	function valid(): vec<classname<B>> {
		return vec[B::class];
	}
}