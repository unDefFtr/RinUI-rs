#include "rinui-gallery/gallery_backend_factory.h"
#include "rinui-gallery/src/backend.cxxqt.h"

std::unique_ptr<rinui_gallery::GalleryBackend> rinui_gallery_backend_new()
{
    return std::make_unique<rinui_gallery::GalleryBackend>();
}
