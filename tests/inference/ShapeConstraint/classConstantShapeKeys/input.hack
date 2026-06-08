final class TracingKeys {
	const string TRACE_HEADER = 'X-Trace-Id';
	const string SPAN_HEADER = 'X-Span-Id';
	const string PASSTHROUGH = 'passthrough';
	const int VERSION = 2;
}

type trace_ctx_t = shape(
	TracingKeys::TRACE_HEADER => string,
	TracingKeys::SPAN_HEADER => string,
	?TracingKeys::PASSTHROUGH => string,
	TracingKeys::VERSION => int,
);

function make_ctx(string $trace_id, string $span_id, ?string $passthrough): trace_ctx_t {
	$ctx = shape(
		TracingKeys::TRACE_HEADER => $trace_id,
		TracingKeys::SPAN_HEADER => $span_id,
		TracingKeys::VERSION => 2,
	);

	if ($passthrough is nonnull) {
		$ctx[TracingKeys::PASSTHROUGH] = $passthrough;
	}

	return $ctx;
}

// literal keys must also match the symbolic declaration
function make_ctx_with_literal_keys(string $trace_id, string $span_id): trace_ctx_t {
	return shape(
		'X-Trace-Id' => $trace_id,
		'X-Span-Id' => $span_id,
		TracingKeys::VERSION => 7,
	);
}

function read_ctx(trace_ctx_t $ctx): string {
	return $ctx[TracingKeys::TRACE_HEADER];
}
