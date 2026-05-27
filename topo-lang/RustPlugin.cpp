#include "RustPlugin.h"

#include "RustAnalyzerBridge.h"
#include "RustAnalysisProvider.h"
#include "RustCheckRunner.h"
#include "RustEmitter.h"

namespace topo::lang {

// -----------------------------------------------------------------------
// EmitterFactory
// -----------------------------------------------------------------------

class RustPlugin::RustEmitterFactory : public EmitterFactory {
public:
    std::unique_ptr<transpile::Emitter> createEmitter() override {
        return std::make_unique<transpile::RustEmitter>();
    }
    std::string fileExtension() const override { return ".rs"; }
};

// -----------------------------------------------------------------------
// BuildDriverFactory
// -----------------------------------------------------------------------

class RustPlugin::RustBuildDriverFactory : public BuildDriverFactory {
public:
    std::string backendToolName() const override { return "topo-build-llvm-rust"; }
    std::string extractorToolName() const override { return "topo-extract-rust"; }
};

// -----------------------------------------------------------------------
// RustPlugin
// -----------------------------------------------------------------------

RustPlugin::RustPlugin()
    : emitterFactory_(std::make_unique<RustEmitterFactory>()),
      buildDriverFactory_(std::make_unique<RustBuildDriverFactory>()) {}

HostLanguage RustPlugin::language() const { return HostLanguage::Rust; }

std::unique_ptr<check::LanguageAnalysisProvider> RustPlugin::createAnalysisProvider() {
    return check::createRustAnalysisProvider();
}

EmitterFactory* RustPlugin::emitterFactory() { return emitterFactory_.get(); }
BuildDriverFactory* RustPlugin::buildDriverFactory() { return buildDriverFactory_.get(); }
InitTemplateProvider* RustPlugin::initTemplateProvider() { return &initProvider_; }

std::unique_ptr<lsp::LSPBridge> RustPlugin::createLSPBridge() {
    return std::make_unique<lsp::RustAnalyzerBridge>();
}

std::unique_ptr<CheckRunnerBase> RustPlugin::createCheckRunner() {
    return std::make_unique<RustCheckRunner>();
}

void registerRustPlugin() {
    registerPlugin(std::make_unique<RustPlugin>());
}

} // namespace topo::lang
