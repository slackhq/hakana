type nested_meta_t = shape(
	'next_cursor' => string,
);

type my_output_t = shape(
	'actions' => vec<string>,
	?'response_metadata' => nested_meta_t,
);

final class MyBuilder {
	protected function getOutputShape(): typename<my_output_t> {
		return my_output_t::class;
	}
}
