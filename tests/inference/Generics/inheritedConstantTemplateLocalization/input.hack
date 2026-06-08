type client_request_t<T> = shape(
	'envelope' => typename<T>,
	'path' => string,
);

final class Client {
	public async function request<T>(client_request_t<T> $request): Awaitable<vec<T>> {
		$_ = await \HH\Asio\usleep(0);
		throw new \Exception('not implemented');
	}
}

abstract class ParentReq<T> {
	abstract const typename<T> ENVELOPE;
}

// a constant declared on a generic ancestor is typed in terms of the
// ancestor's template params — accessing it through the child localizes
// ParentReq::T to ChildReq's T, so the solved return type matches
abstract class ChildReq<T> extends ParentReq<T> {
	public async function send(Client $client): Awaitable<vec<T>> {
		$request = shape(
			'envelope' => static::ENVELOPE,
			'path' => "/foo",
		);
		$res = await $client->request($request);
		return $res;
	}
}
