function test_helper(): void {}

function test_everything(): void {
	soft();
	hard();
	(new Soft())->run();
	test_helper();
}
