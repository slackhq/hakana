<<\Hakana\CallsService('dw')>>
function query_dw(): void {}

<<\Hakana\OnlyUsedInTests>>
final class Iterator {
	<<\Hakana\IndirectlyCallsService('dw')>>
	public function nextPage(): void {
		query_dw();
	}
}

<<\Hakana\IndirectlyCallsService('dw')>>
function unnecessary(): void {}

<<__EntryPoint>>
function main(): void {
	unnecessary();
}
