interface Feature {
	public function getTeam(): string;
	public function name(): string;
}

trait BaseExperiment {
	require implements Feature;

	protected function readExperimentGroup(): string {
		// This should work - Feature has name()
		return $this->name();
	}

	protected abstract function checkExperimentGroup(string $name): string;
}

trait TeamExperiment {
	use BaseExperiment;

	<<__Override>>
	protected function checkExperimentGroup(string $name): string {
		// This should work - BaseExperiment requires Feature which has getTeam()
		return $this->getTeam();
	}
}

final class MyFeature implements Feature {
	use TeamExperiment;

	<<__Override>>
	public function getTeam(): string {
		return "team";
	}

	<<__Override>>
	public function name(): string {
		return "feature";
	}
}
