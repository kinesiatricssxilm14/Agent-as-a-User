package docker

import (
	"context"
	"net/http"
	"net/url"
)

// ListImages returns all images.
func (c *Client) ListImages(ctx context.Context) ([]Image, error) {
	q := url.Values{}
	q.Set("all", "1")
	var out []Image
	if err := c.do(ctx, http.MethodGet, "/images/json", q, nil, &out); err != nil {
		return nil, err
	}
	return out, nil
}

// InspectImage returns detailed image information.
func (c *Client) InspectImage(ctx context.Context, id string) (*ImageInspect, error) {
	var out ImageInspect
	if err := c.do(ctx, http.MethodGet, "/images/"+id+"/json", nil, nil, &out); err != nil {
		return nil, err
	}
	return &out, nil
}

// ImageRemove removes an image.
func (c *Client) ImageRemove(ctx context.Context, id string, force bool) error {
	q := url.Values{}
	if force {
		q.Set("force", "1")
	}
	return c.do(ctx, http.MethodDelete, "/images/"+id, q, nil, nil)
}
