#pragma once

#include <memory>

namespace rinui_gallery {
class GalleryBackend;
}

std::unique_ptr<rinui_gallery::GalleryBackend> rinui_gallery_backend_new();
