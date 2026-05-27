#ifndef TOPO_LANG_RUST_INITTEMPLATEPROVIDER_H
#define TOPO_LANG_RUST_INITTEMPLATEPROVIDER_H

#include "topo/Lang/InitTemplateProvider.h"

namespace topo::lang {

class RustInitTemplateProvider : public InitTemplateProvider {
public:
    std::string languageName() const override { return "rust"; }

    std::vector<std::string> filePatterns() const override {
        return {"*.rs"};
    }

    std::string sourceFileGlob() const override { return "src/**/*.rs"; }

    std::string generateTopoToml(const std::string& projectName) const override;
    std::string generateTypeBindings() const override;

    // Zero-install native-lldb formatter scaffold.
    std::string generateLldbInit(const std::string& formatterRelPath) const override;
    std::string lldbFormatterScript() const override;
    std::string lldbFormatterRelPath() const override;
};

} // namespace topo::lang

#endif // TOPO_LANG_RUST_INITTEMPLATEPROVIDER_H
