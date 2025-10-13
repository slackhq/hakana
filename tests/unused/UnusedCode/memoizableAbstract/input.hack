abstract class MemoizableParent implements IMemoizeParam {
    <<__Override>>
    public function getInstanceKey(): string {
		return 'a';
	}
}

final class MemoizableChild extends MemoizableParent {
}

<<__EntryPoint>>
function main(): void {
    new MemoizableChild();
}
