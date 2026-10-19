function test_calls(): void {
	echo discarded(2);
	echo \HH\Asio\join(discarded_async(2));
	echo \HH\Asio\join(joined());
	echo only_test_calls(2);
}
