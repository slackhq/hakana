interface MyInvResult<Tv> {}

final class MyInvResultImpl<Tv> implements MyInvResult<Tv> {
	public function __construct(private Tv $v) {}
}

function inv_result<Tvv>(Tvv $v): MyInvResult<Tvv> {
	return new MyInvResultImpl($v);
}

interface Animal {}
final class Dog implements Animal {}

function literal_bool_case(): MyInvResult<bool> {
	return inv_result(true);
}

function empty_vec_case(): MyInvResult<vec<string>> {
	return inv_result(vec[]);
}

function subclass_case(): MyInvResult<Animal> {
	return inv_result(new Dog());
}

function null_case(): MyInvResult<?Dog> {
	return inv_result(null);
}

function dict_literal_case(): MyInvResult<dict<string, string>> {
	return inv_result(dict['a' => 'b']);
}
