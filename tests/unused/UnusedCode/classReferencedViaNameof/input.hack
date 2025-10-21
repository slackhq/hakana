use Imported\SomeNamespace\UsedViaUseStatement;

final class ThisIsAlsoUsed {}

final class ThisIsUsedAsWell {}

final class ThisIsAlsoUnused {}

<<__EntryPoint>>
function main(): void {
    echo nameof N\ThisIsUsed;
    echo nameof ThisIsAlsoUsed;
    echo nameof UsedViaUseStatement;

    N\foo();
}