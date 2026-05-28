final class RootEg {
	<<\Hakana\MustUse>>
	public static function apiList(): ApiListEg {
		return new ApiListEg();
	}
}

trait ApiMethodTrait {
	<<\Hakana\MustUse>>
	public function apiMethod(): ApiMethodEg {
		return new ApiMethodEg();
	}
}

final class ApiListEg {
	use ApiMethodTrait;
}

abstract class ApiBuilderEg {
	<<\Hakana\MustUse>>
	public final function getOk(): string {
		return 'ok';
	}
}

final class ApiMethodEg extends ApiBuilderEg {
	<<\Hakana\MustUse>>
	public function arg(): ApiMethodEg {
		return $this;
	}

}

function main(): void {

	RootEg::apiList();

	RootEg::apiList()->apiMethod();

	RootEg::apiList()->apiMethod()->arg();

	RootEg::apiList()->apiMethod()->arg()->getOk();

	RootEg::apiList()->apiMethod()->arg()->arg()->getOk();

}
