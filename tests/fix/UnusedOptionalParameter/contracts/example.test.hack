function test(TestImplementation $implementation): void {
	echo tested(2);
	echo $implementation->value(2);
}
