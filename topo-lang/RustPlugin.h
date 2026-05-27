#ifndef TOPO_LANG_RUSTPLUGIN_H
#define TOPO_LANG_RUSTPLUGIN_H

#include "topo/Lang/LanguagePlugin.h"
#include "topo/Lang/CheckRunnerBase.h"
#include "topo/Lang/EmitterFactory.h"
#include "topo/Lang/BuildDriverFactory.h"
#include "RustInitTemplateProvider.h"

namespace topo::lang {

class RustPlugin : public LanguagePlugin {
public:
    RustPlugin();

    HostLanguage language() const override;
    std::unique_ptr<check::LanguageAnalysisProvider> createAnalysisProvider() override;
    EmitterFactory* emitterFactory() override;
    BuildDriverFactory* buildDriverFactory() override;
    InitTemplateProvider* initTemplateProvider() override;
    std::unique_ptr<lsp::LSPBridge> createLSPBridge() override;
    std::unique_ptr<CheckRunnerBase> createCheckRunner() override;

private:
    class RustEmitterFactory;
    class RustBuildDriverFactory;
    std::unique_ptr<RustEmitterFactory> emitterFactory_;
    std::unique_ptr<RustBuildDriverFactory> buildDriverFactory_;
    RustInitTemplateProvider initProvider_;
};

/// Call once at startup to register the Rust plugin.
void registerRustPlugin();

} // namespace topo::lang

#endif // TOPO_LANG_RUSTPLUGIN_H
